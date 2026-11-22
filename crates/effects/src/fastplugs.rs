//! Offline CLAP host. Call only inside the Sonitrelix plug-in helper process.
use clap_sys::{
    audio_buffer::clap_audio_buffer,
    entry::clap_plugin_entry,
    events::*,
    ext::{audio_ports::*, latency::*, params::*},
    factory::plugin_factory::*,
    host::clap_host,
    plugin::clap_plugin,
    process::*,
    version::CLAP_VERSION,
};
use libloading::os::windows::{
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library,
};
use serde::{Deserialize, Serialize};
use sonitrelix_audio_io::{WavInfo, WavReader, WavWriter};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    io,
    mem::{size_of, zeroed},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub id: u32,
    pub name: String,
    pub current: String,
    pub min: String,
    pub max: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Effect {
    pub bundle: PathBuf,
    pub id: String,
    pub changes: Vec<(u32, String)>,
}

fn missing(component: &str) -> io::Error {
    io::Error::other(format!("FastPlugs did not provide {component}"))
}

struct Bundle {
    _library: Library,
    entry: clap_plugin_entry,
}
impl Bundle {
    fn load(path: &Path) -> io::Result<Self> {
        let path = path.canonicalize()?;
        let library = unsafe {
            Library::load_with_flags(
                &path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        }
        .map_err(io::Error::other)?;
        let entry = unsafe {
            let symbol = library
                .get::<*const clap_plugin_entry>(b"clap_entry\0")
                .map_err(io::Error::other)?;
            **symbol
        };
        if entry.clap_version.major != CLAP_VERSION.major {
            return Err(io::Error::other("Unsupported CLAP version"));
        }
        let path_utf8 =
            CString::new(path.to_string_lossy().as_bytes()).map_err(io::Error::other)?;
        if !unsafe {
            entry.init.ok_or_else(|| missing("bundle initialization"))?(path_utf8.as_ptr())
        } {
            return Err(io::Error::other("FastPlugs could not initialize"));
        }
        Ok(Self {
            _library: library,
            entry,
        })
    }
    fn factory(&self) -> io::Result<&clap_plugin_factory> {
        unsafe {
            self.entry
                .get_factory
                .ok_or_else(|| missing("plug-in factory"))?(
                CLAP_PLUGIN_FACTORY_ID.as_ptr()
            )
            .cast::<clap_plugin_factory>()
            .as_ref()
            .ok_or_else(|| missing("plug-in factory"))
        }
    }
    fn instance(&self, id: &str) -> io::Result<Instance<'_>> {
        let id = CString::new(id).map_err(io::Error::other)?;
        let factory = self.factory()?;
        let host = Box::new(clap_host {
            clap_version: CLAP_VERSION,
            host_data: null_mut(),
            name: c"Sonitrelix".as_ptr(),
            vendor: c"Sonitrelix".as_ptr(),
            url: c"https://github.com/AnimalMetal/sonitrelix".as_ptr(),
            version: c"1".as_ptr(),
            get_extension: Some(host_extension),
            request_restart: Some(host_request),
            request_process: Some(host_request),
            request_callback: Some(host_request),
        });
        unsafe {
            let plugin = factory
                .create_plugin
                .ok_or_else(|| missing("plug-in constructor"))?(
                factory, &*host, id.as_ptr()
            )
            .as_ref()
            .ok_or_else(|| missing("requested plug-in"))?;
            let instance = Instance {
                plugin,
                _host: host,
                active: false,
                processing: false,
            };
            if !plugin
                .init
                .ok_or_else(|| missing("plug-in initialization"))?(plugin)
            {
                return Err(io::Error::other("Plug-in initialization failed"));
            }
            Ok(instance)
        }
    }
}
impl Drop for Bundle {
    fn drop(&mut self) {
        if let Some(deinit) = self.entry.deinit {
            unsafe { deinit() };
        }
    }
}
unsafe extern "C" fn host_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    null()
}
unsafe extern "C" fn host_request(_: *const clap_host) {}

struct Instance<'a> {
    plugin: &'a clap_plugin,
    _host: Box<clap_host>,
    active: bool,
    processing: bool,
}
impl Drop for Instance<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.processing
                && let Some(stop) = self.plugin.stop_processing
            {
                stop(self.plugin);
            }
            if self.active
                && let Some(deactivate) = self.plugin.deactivate
            {
                deactivate(self.plugin);
            }
            if let Some(destroy) = self.plugin.destroy {
                destroy(self.plugin);
            }
        }
    }
}
fn fixed_text(data: &[c_char]) -> String {
    let bytes: Vec<_> = data
        .iter()
        .take_while(|c| **c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
fn supported(id: &str) -> bool {
    id.starts_with("io.github.masonasons.fastplugs.")
        && !["buffer", "clipping-monitor", "clip-monitor"]
            .contains(&id.rsplit('.').next().unwrap_or(""))
}
impl Instance<'_> {
    unsafe fn extension<T>(&self, id: &CStr) -> Option<&T> {
        unsafe {
            (self.plugin.get_extension?)(self.plugin, id.as_ptr())
                .cast::<T>()
                .as_ref()
        }
    }
    fn parameters(&self) -> Vec<(clap_param_info, Parameter)> {
        unsafe {
            let Some(params) = self.extension::<clap_plugin_params>(CLAP_EXT_PARAMS) else {
                return vec![];
            };
            let (Some(count), Some(get_info)) = (params.count, params.get_info) else {
                return vec![];
            };
            (0..count(self.plugin).min(256))
                .filter_map(|index| {
                    let mut info: clap_param_info = zeroed();
                    if !get_info(self.plugin, index, &mut info)
                        || info.flags & (CLAP_PARAM_IS_HIDDEN | CLAP_PARAM_IS_READONLY) != 0
                        || !info.min_value.is_finite()
                        || !info.max_value.is_finite()
                        || info.max_value < info.min_value
                    {
                        return None;
                    }
                    let format = |value: f64| {
                        let mut buffer = [0; 256];
                        if params.value_to_text.is_some_and(|format| {
                            format(
                                self.plugin,
                                info.id,
                                value,
                                buffer.as_mut_ptr(),
                                buffer.len() as u32,
                            )
                        }) {
                            fixed_text(&buffer)
                        } else {
                            value.to_string()
                        }
                    };
                    let mut value = info.default_value;
                    if let Some(get) = params.get_value {
                        get(self.plugin, info.id, &mut value);
                    }
                    let name = fixed_text(&info.name);
                    let group = fixed_text(&info.module);
                    let name = if group.is_empty() {
                        name
                    } else {
                        format!("{group}, {name}")
                    };
                    Some((
                        info,
                        Parameter {
                            id: info.id,
                            name,
                            current: format(value),
                            min: format(info.min_value),
                            max: format(info.max_value),
                        },
                    ))
                })
                .collect()
        }
    }
    fn apply_parameters(&self, changes: &[(u32, String)]) -> io::Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let available = self.parameters();
        unsafe {
            let extension = self
                .extension::<clap_plugin_params>(CLAP_EXT_PARAMS)
                .ok_or_else(|| missing("parameters"))?;
            let flush = extension
                .flush
                .ok_or_else(|| missing("parameter updates"))?;
            for (id, text) in changes {
                let Some((info, parameter)) = available.iter().find(|(info, _)| info.id == *id)
                else {
                    return Err(io::Error::other(
                        "The selected plug-in parameter no longer exists",
                    ));
                };
                let label = CString::new(text.as_str()).map_err(io::Error::other)?;
                let mut value = f64::NAN;
                if !extension
                    .text_to_value
                    .is_some_and(|parse| parse(self.plugin, *id, label.as_ptr(), &mut value))
                {
                    value = text.trim().replace(',', ".").parse().unwrap_or(f64::NAN);
                }
                if !value.is_finite() || !(info.min_value..=info.max_value).contains(&value) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "{}: {} ({} to {})",
                            parameter.name, text, parameter.min, parameter.max
                        ),
                    ));
                }
                let mut event = clap_event_param_value {
                    header: clap_event_header {
                        size: size_of::<clap_event_param_value>() as u32,
                        time: 0,
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: CLAP_EVENT_PARAM_VALUE,
                        flags: 0,
                    },
                    param_id: *id,
                    cookie: info.cookie,
                    note_id: -1,
                    port_index: -1,
                    channel: -1,
                    key: -1,
                    value,
                };
                let events = clap_input_events {
                    ctx: (&mut event as *mut clap_event_param_value).cast(),
                    size: Some(event_count),
                    get: Some(event_get),
                };
                flush(self.plugin, &events, &OUTPUT_EVENTS);
            }
        }
        Ok(())
    }
    fn channels(&self, input: bool) -> io::Result<usize> {
        unsafe {
            let ports = self
                .extension::<clap_plugin_audio_ports>(CLAP_EXT_AUDIO_PORTS)
                .ok_or_else(|| missing("audio ports"))?;
            let count = ports.count.ok_or_else(|| missing("audio port count"))?;
            let get = ports.get.ok_or_else(|| missing("audio port information"))?;
            let mut info: clap_audio_port_info = zeroed();
            if count(self.plugin, input) != 1
                || !get(self.plugin, 0, input, &mut info)
                || ![1, 2].contains(&info.channel_count)
            {
                return Err(io::Error::other(
                    "Only one mono or stereo plug-in bus is supported",
                ));
            }
            Ok(info.channel_count as usize)
        }
    }
}
unsafe extern "C" fn event_count(list: *const clap_input_events) -> u32 {
    u32::from(unsafe { !(*list).ctx.is_null() })
}
unsafe extern "C" fn event_get(
    list: *const clap_input_events,
    index: u32,
) -> *const clap_event_header {
    if index == 0 {
        unsafe { (*list).ctx.cast() }
    } else {
        null()
    }
}
unsafe extern "C" fn discard_event(
    _: *const clap_output_events,
    _: *const clap_event_header,
) -> bool {
    true
}
const NO_EVENTS: clap_input_events = clap_input_events {
    ctx: null_mut(),
    size: Some(event_count),
    get: Some(event_get),
};
const OUTPUT_EVENTS: clap_output_events = clap_output_events {
    ctx: null_mut(),
    try_push: Some(discard_event),
};

pub fn scan(path: &Path) -> io::Result<Vec<Plugin>> {
    let bundle = Bundle::load(path)?;
    let factory = bundle.factory()?;
    let mut plugins = Vec::new();
    unsafe {
        let count = factory
            .get_plugin_count
            .ok_or_else(|| missing("plug-in count"))?(factory);
        let get = factory
            .get_plugin_descriptor
            .ok_or_else(|| missing("plug-in descriptions"))?;
        for index in 0..count.min(128) {
            let Some(desc) = get(factory, index).as_ref() else {
                continue;
            };
            if desc.id.is_null() || desc.name.is_null() {
                continue;
            }
            let id = CStr::from_ptr(desc.id).to_string_lossy().into_owned();
            let name = CStr::from_ptr(desc.name).to_string_lossy().into_owned();
            if !supported(&id) || ["Buffer", "Clipping Monitor"].contains(&name.as_str()) {
                continue;
            }
            let parameters = bundle
                .instance(&id)?
                .parameters()
                .into_iter()
                .map(|(_, param)| param)
                .collect();
            plugins.push(Plugin {
                id,
                name,
                parameters,
            });
        }
    }
    if plugins.is_empty() {
        return Err(io::Error::other(
            "This CLAP bundle has no FastPlugs audio effects",
        ));
    }
    Ok(plugins)
}

pub fn render(effect: &Effect, input: &Path, output: &Path) -> io::Result<WavInfo> {
    if !supported(&effect.id) {
        return Err(io::Error::other("Unsupported FastPlugs effect"));
    }
    const BLOCK: usize = 1024;
    let mut reader = WavReader::open(input)?;
    let format = reader.info;
    let channels = usize::from(format.channels);
    let bundle = Bundle::load(&effect.bundle)?;
    let mut instance = bundle.instance(&effect.id)?;
    instance.apply_parameters(&effect.changes)?;
    let inputs = instance.channels(true)?;
    let outputs = instance.channels(false)?;
    let plugin = instance.plugin;
    let mut source = vec![0.; BLOCK * channels];
    let mut result = vec![0.; BLOCK * channels];
    let mut port_in = [vec![0.; BLOCK], vec![0.; BLOCK]];
    let mut port_out = [vec![0.; BLOCK], vec![0.; BLOCK]];
    unsafe {
        let activate = plugin.activate.ok_or_else(|| missing("audio activation"))?;
        if !activate(plugin, f64::from(format.sample_rate), 1, BLOCK as u32) {
            return Err(io::Error::other(
                "This FastPlugs effect could not open the track sample rate",
            ));
        }
        instance.active = true;
        let latency = instance
            .extension::<clap_plugin_latency>(CLAP_EXT_LATENCY)
            .and_then(|extension| extension.get)
            .map_or(0, |get| u64::from(get(plugin)));
        if latency > u64::from(format.sample_rate) * 15 {
            return Err(io::Error::other(
                "Plug-in latency exceeds the offline renderer limit",
            ));
        }
        let start = plugin
            .start_processing
            .ok_or_else(|| missing("start processing"))?;
        let process = plugin.process.ok_or_else(|| missing("audio processing"))?;
        if !start(plugin) {
            return Err(io::Error::other("FastPlugs refused to start processing"));
        }
        instance.processing = true;
        let mut writer = WavWriter::create_float(output, format.sample_rate, format.channels)?;
        let total = format.frames + latency;
        let mut position = 0;
        while position < total {
            let count = (total - position).min(BLOCK as u64) as usize;
            source.fill(0.);
            reader.read_frames(&mut source[..count * channels])?;
            for i in 0..count {
                let a = source[i * channels];
                let b = source[i * channels + channels - 1];
                port_in[0][i] = if inputs == 1 { (a + b) * 0.5 } else { a };
                port_in[1][i] = b;
            }
            port_out[0].fill(0.);
            port_out[1].fill(0.);
            let mut in_ptr = port_in.each_mut().map(|v| v.as_mut_ptr());
            let mut out_ptr = port_out.each_mut().map(|v| v.as_mut_ptr());
            let bus = |data32, count| clap_audio_buffer {
                data32,
                data64: null_mut(),
                channel_count: count as u32,
                latency: 0,
                constant_mask: 0,
            };
            let input = bus(in_ptr.as_mut_ptr(), inputs);
            let mut output = bus(out_ptr.as_mut_ptr(), outputs);
            let request = clap_process {
                steady_time: position as i64,
                frames_count: count as u32,
                transport: null(),
                audio_inputs: &input,
                audio_outputs: &mut output,
                audio_inputs_count: 1,
                audio_outputs_count: 1,
                in_events: &NO_EVENTS,
                out_events: &OUTPUT_EVENTS,
            };
            if process(plugin, &request) == CLAP_PROCESS_ERROR {
                return Err(io::Error::other("FastPlugs could not process the audio"));
            }
            let skip = latency.saturating_sub(position).min(count as u64) as usize;
            let length = count - skip;
            for i in 0..length {
                let a = port_out[0][i + skip];
                let b = port_out[outputs - 1][i + skip];
                result[i * channels] = if channels == 1 { (a + b) * 0.5 } else { a };
                if channels == 2 {
                    result[i * channels + 1] = b;
                }
            }
            writer.write_frames(&result[..length * channels])?;
            position += count as u64;
        }
        writer.finish()
    }
}

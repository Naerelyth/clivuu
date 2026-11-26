//! Audio devices exposed by PulseAudio or PipeWire's PulseAudio service.
//! Probe only this provider, never ALSA/camera providers or a physical device.
use gst::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct Devices {
    pub system: Option<String>,
    pub microphone: Option<String>,
}
#[derive(Debug)]
struct Device {
    name: String,
    class: String,
    kind: String,
    default: bool,
}
impl Devices {
    /// Called on the bounded worker, probing lists devices without recording.
    pub fn detect() -> Self {
        if gst::init().is_err() {
            return Self::default();
        }
        let Some(provider) = gst::DeviceProviderFactory::by_name("pulsedeviceprovider") else {
            return Self::default();
        };
        // The one-shot probe does not populate Pulse's default source/sink.
        // Start this provider only, it lists server state without opening audio.
        if provider.start().is_err() {
            return Self::default();
        }
        struct Stop(gst::DeviceProvider);
        impl Drop for Stop {
            fn drop(&mut self) {
                self.0.stop();
            }
        }
        let _stop = Stop(provider.clone());
        let devices = provider
            .devices()
            .into_iter()
            .filter_map(|device| {
                let properties = device.properties()?;
                Some(Device {
                    name: device.property::<String>("internal-name"),
                    class: device.device_class().to_string(),
                    kind: properties.get::<String>("device.class").unwrap_or_default(),
                    default: properties.get::<bool>("is-default").unwrap_or(false),
                })
            })
            .collect::<Vec<_>>();
        Self::from_devices(&devices)
    }
    fn from_devices(devices: &[Device]) -> Self {
        let sink = devices.iter().find(|d| {
            d.class == "Audio/Sink" && d.kind != "abstract" && d.name != "auto_null" && d.default
        });
        let system = sink
            .and_then(|sink| {
                devices.iter().find(|d| {
                    d.class == "Audio/Source" && d.name == format!("{}.monitor", sink.name)
                })
            })
            .map(|d| d.name.clone());
        let microphone = devices
            .iter()
            .find(|d| {
                d.class == "Audio/Source"
                    && d.kind != "monitor"
                    && d.kind != "abstract"
                    && !d.name.ends_with(".monitor")
                    && d.default
            })
            .map(|d| d.name.clone());
        Self { system, microphone }
    }
    pub fn selected(&self, system: bool, microphone: bool) -> Vec<String> {
        self.system
            .iter()
            .filter(|_| system)
            .chain(self.microphone.iter().filter(|_| microphone))
            .cloned()
            .collect()
    }
    pub fn availability(&self) -> clivuu_core::recording::AudioAvailability {
        clivuu_core::recording::AudioAvailability {
            system: self.system.is_some(),
            microphone: self.microphone.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn device(name: &str, class: &str, kind: &str, default: bool) -> Device {
        Device {
            name: name.into(),
            class: class.into(),
            kind: kind.into(),
            default,
        }
    }
    #[test]
    fn defaults_distinguish_output_monitors_microphones_and_missing_hardware() {
        let empty = Devices::from_devices(&[
            device("auto_null", "Audio/Sink", "", true),
            device("auto_null.monitor", "Audio/Source", "monitor", true),
        ]);
        assert!(empty.system.is_none() && empty.microphone.is_none());
        let devices = Devices::from_devices(&[
            device("old", "Audio/Sink", "sound", false),
            device("old.monitor", "Audio/Source", "monitor", false),
            device("speakers", "Audio/Sink", "sound", true),
            device("speakers.monitor", "Audio/Source", "monitor", false),
            device("other-mic", "Audio/Source", "sound", false),
            device("mic", "Audio/Source", "sound", true),
        ]);
        assert_eq!(devices.selected(true, true), ["speakers.monitor", "mic"]);
        assert_eq!(devices.selected(false, true), ["mic"]);
        assert_eq!(devices.selected(true, false), ["speakers.monitor"]);
    }
}

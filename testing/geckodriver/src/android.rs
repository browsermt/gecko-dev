use crate::capabilities::{AndroidOptions};
use mozdevice::{Device, Host};
use mozprofile::profile::Profile;
use serde::Serialize;
use serde_yaml::{Mapping, Value};
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time;

// TODO: avoid port clashes across GeckoView-vehicles.
// For now, we always use target port 2829, leading to issues like bug 1533704.
const TARGET_PORT: u16 = 2829;

const CONFIG_FILE_HEADING: &str =
r#"## GeckoView configuration YAML
##
## Auto-generated by geckodriver.
## See https://mozilla.github.io/geckoview/consumer/docs/automation.
"#;

pub type Result<T> = std::result::Result<T, AndroidError>;

#[derive(Debug)]
pub enum AndroidError {
    ActivityNotFound(String),
    Device(mozdevice::DeviceError),
    IO(io::Error),
    NotConnected,
    Serde(serde_yaml::Error),
}

impl fmt::Display for AndroidError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AndroidError::ActivityNotFound(ref package) => {
                write!(f, "Activity not found for package '{}'", package)
            },
            AndroidError::Device(ref message) => message.fmt(f),
            AndroidError::IO(ref message) => message.fmt(f),
            AndroidError::NotConnected =>
                write!(f, "Not connected to any Android device"),
            AndroidError::Serde(ref message) => message.fmt(f),
        }

    }
}

impl From<io::Error> for AndroidError {
    fn from(value: io::Error) -> AndroidError {
        AndroidError::IO(value)
    }
}

impl From<mozdevice::DeviceError> for AndroidError {
    fn from(value: mozdevice::DeviceError) -> AndroidError {
        AndroidError::Device(value)
    }
}

impl From<serde_yaml::Error> for AndroidError {
    fn from(value: serde_yaml::Error) -> AndroidError {
        AndroidError::Serde(value)
    }
}

/// A remote Gecko instance.
///
/// Host refers to the device running `geckodriver`.  Target refers to the
/// Android device running Gecko in a GeckoView-based vehicle.
#[derive(Debug)]
pub struct AndroidProcess {
    pub device: Device,
    pub package: String,
    pub activity: String,
}

impl AndroidProcess {
    pub fn new(
        device: Device,
        package: String,
        activity: String,
    ) -> mozdevice::Result<AndroidProcess> {
        Ok(AndroidProcess { device, package, activity })
    }
}

#[derive(Debug, Default)]
pub struct AndroidHandler {
    pub options: AndroidOptions,
    pub process: Option<AndroidProcess>,
    pub profile: PathBuf,

    // For port forwarding host => target
    pub host_port: u16,
    pub target_port: u16,
}

impl Drop for AndroidHandler {
    fn drop(&mut self) {
        // Try to clean up various settings
        if let Some(ref process) = self.process {
            let clear_command = format!("am clear-debug-app {}", process.package);
            match process.device.execute_host_shell_command(&clear_command) {
                Ok(_) => debug!("Disabled reading from configuration file"),
                Err(e) => error!("Failed disabling from configuration file: {}", e),
            }

            match process.device.kill_forward_port(self.host_port) {
                Ok(_) => debug!("Android port forward ({} -> {}) stopped",
                                &self.host_port, &self.target_port),
                Err(e) => error!("Android port forward ({} -> {}) failed to stop: {}",
                                 &self.host_port, &self.target_port, e),
            }
        }
    }
}

impl AndroidHandler {
    pub fn new(options: &AndroidOptions) -> AndroidHandler {
        // We need to push profile.pathbuf to a safe space on the device.
        // Make it per-Android package to avoid clashes and confusion.
        // This naming scheme follows GeckoView's configuration file naming scheme,
        // see bug 1533385.
        let profile = PathBuf::from(format!(
            "/mnt/sdcard/{}-geckodriver-profile", &options.package));

        AndroidHandler {
            options: options.clone(),
            profile,
            process: None,
            ..Default::default()
        }
    }

    pub fn connect(&mut self, host_port: u16) -> Result<()> {
        let host = Host {
            host: None,
            port: None,
            read_timeout: Some(time::Duration::from_millis(5000)),
            write_timeout: Some(time::Duration::from_millis(5000)),
        };

        let device = host.device_or_default(self.options.device_serial.as_ref())?;

        self.host_port = host_port;
        self.target_port = TARGET_PORT;

        // Set up port forward.  Port forwarding will be torn down, if possible,
        device.forward_port(self.host_port, self.target_port)?;
        debug!("Android port forward ({} -> {}) started", &self.host_port, &self.target_port);

        // If activity hasn't been specified default to the main activity of the package
        let activity = match self.options.activity {
            Some(ref activity) => activity.clone(),
            None => {
                let response = device.execute_host_shell_command(&format!(
                    "cmd package resolve-activity --brief {} | tail -n 1",
                    &self.options.package))?;
                let parts = response
                    .trim_end()
                    .split('/')
                    .collect::<Vec<&str>>();

                if parts.len() == 1 {
                    return Err(AndroidError::ActivityNotFound(self.options.package.clone()));
                }

                parts[1].to_owned()
            }
        };

        self.process = Some(AndroidProcess::new(
            device,
            self.options.package.clone(),
            activity,
        )?);

        Ok(())
    }

    pub fn generate_config_file<I, K, V>(&self, envs: I) -> Result<String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: ToString,
        V: ToString
    {
        // To configure GeckoView, we use the automation techniques documented at
        // https://mozilla.github.io/geckoview/consumer/docs/automation.
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        pub struct Config {
            pub env: Mapping,
            pub args: Value,
        }

        // TODO: Allow to write custom arguments and preferences from moz:firefoxOptions
        let mut config = Config {
            args: Value::Sequence(vec![
                Value::String("-marionette".into()),
                Value::String("-profile".into()),
                Value::String(self.profile.display().to_string()),
            ]),
            env: Mapping::new(),
        };

        for (key, value) in envs {
            config.env.insert(
                Value::String(key.to_string()),
                Value::String(value.to_string()),
            );
        }

        config.env.insert(
            Value::String("MOZ_CRASHREPORTER".to_owned()),
            Value::String("1".to_owned()),
        );
        config.env.insert(
            Value::String("MOZ_CRASHREPORTER_NO_REPORT".to_owned()),
            Value::String("1".to_owned()),
        );
        config.env.insert(
            Value::String("MOZ_CRASHREPORTER_SHUTDOWN".to_owned()),
            Value::String("1".to_owned()),
        );

        let mut contents: Vec<String> = vec!(CONFIG_FILE_HEADING.to_owned());
        contents.push(serde_yaml::to_string(&config)?);

        Ok(contents.concat())
    }

    pub fn prepare<I, K, V>(&self, profile: &Profile, env: I) -> Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: ToString,
        V: ToString
    {
        match self.process {
            Some(ref process) => {
                process.device.clear_app_data(&process.package)?;

                // These permissions, at least, are required to read profiles in /mnt/sdcard.
                for perm in &["READ_EXTERNAL_STORAGE", "WRITE_EXTERNAL_STORAGE"] {
                    process.device.execute_host_shell_command(&format!(
                        "pm grant {} android.permission.{}", &process.package, perm))?;
                }

                debug!("Deleting {}", self.profile.display());
                process.device.execute_host_shell_command(&format!(
                    "rm -rf {}", self.profile.display()))?;

                debug!("Pushing {} to {}", profile.path.display(), self.profile.display());
                process.device.push_dir(&profile.path, &self.profile, 0o777)?;

                // Pushing GeckoView configuration file to the device
                let mut target_path = PathBuf::from("/data/local/tmp");
                target_path.push(&format!("{}-geckoview-config.yaml", process.package));

                let contents = self.generate_config_file(env)?;
                debug!("Content of generated GeckoView config file:\n{}", contents);
                let reader = &mut io::BufReader::new(contents.as_bytes());

                debug!("Pushing GeckoView configuration file to {}", target_path.display());
                process.device.push(reader, &target_path, 0o777)?;

                // Bug 1584966: File permissions are not correctly set by push()
                process.device.execute_host_shell_command(&format!(
                    "chmod a+rw {}",
                    target_path.display()))?;

                // Tell GeckoView to read configuration even when `android:debuggable="false"`.
                process.device.execute_host_shell_command(&format!(
                    "am set-debug-app --persistent {}",
                    process.package))?;
            },
            None => return Err(AndroidError::NotConnected)
        }

        Ok(())
    }

    pub fn launch(&self) -> Result<()> {
        match self.process {
            Some(ref process) => {
                // TODO: Remove the usage of intent arguments once Fennec is no longer
                // supported. Packages which are using GeckoView always read the arguments
                // via the YAML configuration file.
                let mut intent_arguments = self.options.intent_arguments.clone()
                    .unwrap_or_else(|| Vec::with_capacity(3));
                intent_arguments.push("--es".to_owned());
                intent_arguments.push("args".to_owned());
                intent_arguments.push(format!(
                    "-marionette -profile {}", self.profile.display()).to_owned());

                debug!("Launching {}/{}", process.package, process.activity);
                process.device
                    .launch(&process.package, &process.activity, &intent_arguments)
                    .map_err(|e| {
                        let message = format!(
                            "Could not launch Android {}/{}: {}", process.package, process.activity, e);
                        mozdevice::DeviceError::Adb(message)
                    })?;
            },
            None => return Err(AndroidError::NotConnected)
        }

        Ok(())
    }

    pub fn force_stop(&self) -> Result<()> {
        match &self.process {
            Some(process) => {
                debug!("Force stopping the Android package: {}", &process.package);
                process.device.force_stop(&process.package)?;
            },
            None => return Err(AndroidError::NotConnected)
        }

        Ok(())
   }
}

use std::collections::BTreeMap;
use std::fmt::Write;

use camino::Utf8PathBuf;

#[derive(Debug, Clone, Default)]
pub struct LaunchdPlist {
    label: String,
    program: Option<String>,
    program_arguments: Option<Vec<String>>,
    environment_variables: Option<BTreeMap<String, String>>,
    standard_in_path: Option<String>,
    standard_out_path: Option<String>,
    standard_error_path: Option<String>,
    working_directory: Option<String>,
    run_at_load: Option<bool>,
    keep_alive: Option<bool>,
    throttle_interval: Option<i64>,
    watch_paths: Option<Vec<String>>,
    associated_bundle_identifiers: Option<Vec<String>>,
}

impl LaunchdPlist {
    pub fn new(label: impl Into<String>) -> LaunchdPlist {
        LaunchdPlist {
            label: label.into(),
            ..LaunchdPlist::default()
        }
    }

    pub fn get_file_path(&self) -> Result<Utf8PathBuf, super::Error> {
        let path = super::directories::home_dir_utf8()?
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", self.label));

        Ok(path)
    }

    /// Generate the plist as a string
    pub fn plist(&self) -> String {
        let mut plist = String::new();

        let indent = "    ";
        let mut indent_level = 0;

        macro_rules! indent_block {
            ($block:block) => {{
                {
                    indent_level += 1;
                    $block;
                    indent_level -= 1;
                }
            }};
        }

        macro_rules! push_line {
            ($($arg:tt)*) => {{
                for _ in 0..indent_level {
                    plist.push_str(indent);
                }
                writeln!(plist, $($arg)*).ok();
            }};
        }

        macro_rules! push_key_val {
            ($key:expr,String, $val:expr) => {{
                push_line!("<key>{}</key>", $key);
                push_line!("<string>{}</string>", $val);
            }};
            ($key:expr, &[String], $val:expr) => {{
                push_line!("<key>{}</key>", $key);
                push_line!("<array>");
                indent_block!({
                    for s in $val.iter() {
                        push_line!("<string>{}</string>", s);
                    }
                });
                push_line!("</array>");
            }};
            ($key:expr,bool, $val:expr) => {{
                push_line!("<key>{}</key>", $key);
                push_line!("{}", if $val { "<true/>" } else { "<false/>" });
            }};
            ($key:expr,i64, $val:expr) => {{
                push_line!("<key>{}</key>", $key);
                push_line!("<integer>{}</integer>", $val);
            }};
            ($key:expr,HashMap < String, $t:tt > , $val:expr) => {{
                push_line!("<key>{}</key>", $key);
                push_line!("<dict>");
                indent_block!({
                    for (k, v) in $val.iter() {
                        push_key_val!(k, $t, v);
                    }
                });
                push_line!("</dict>");
            }};
        }

        push_line!(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        push_line!(
            r#"<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">"#
        );
        push_line!(r#"<plist version="1.0">"#);

        indent_block!({
            push_line!("<dict>");

            indent_block!({
                push_key_val!("Label", String, &self.label);

                if let Some(program) = &self.program {
                    push_key_val!("Program", String, &program);
                }

                if let Some(program_arguments) = &self.program_arguments {
                    push_key_val!("ProgramArguments", &[String], program_arguments);
                }

                if let Some(environment_variables) = &self.environment_variables {
                    push_key_val!(
                        "EnvironmentVariables",
                        HashMap<String, String>,
                        environment_variables
                    );
                }

                if let Some(standard_in_path) = &self.standard_in_path {
                    push_key_val!("StandardInPath", String, &standard_in_path);
                }

                if let Some(standard_out_path) = &self.standard_out_path {
                    push_key_val!("StandardOutPath", String, &standard_out_path);
                }

                if let Some(standard_error_path) = &self.standard_error_path {
                    push_key_val!("StandardErrorPath", String, &standard_error_path);
                }

                if let Some(working_directory) = &self.working_directory {
                    push_key_val!("WorkingDirectory", String, &working_directory);
                }

                if let Some(run_at_load) = &self.run_at_load {
                    push_key_val!("RunAtLoad", bool, *run_at_load);
                }

                if let Some(keep_alive) = &self.keep_alive {
                    push_key_val!("KeepAlive", bool, *keep_alive);
                }

                if let Some(throttle_interval) = &self.throttle_interval {
                    push_key_val!("ThrottleInterval", i64, *throttle_interval);
                }

                if let Some(watch_paths) = &self.watch_paths {
                    push_key_val!("WatchPaths", &[String], watch_paths);
                }

                if let Some(associated_bundle_identifiers) = &self.associated_bundle_identifiers {
                    push_key_val!("AssociatedBundleIdentifiers", &[String], associated_bundle_identifiers);
                }
            });

            push_line!("</dict>");
        });

        push_line!("</plist>");

        plist
    }

    /// Set the label
    pub fn _label(mut self, label: impl Into<String>) -> LaunchdPlist {
        self.label = label.into();
        self
    }

    /// Set the program
    pub fn program(mut self, program: impl Into<String>) -> LaunchdPlist {
        self.program = Some(program.into());
        self
    }

    /// Set the program arguments
    pub fn program_arguments<I, T>(mut self, program_arguments: I) -> LaunchdPlist
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.program_arguments = Some(program_arguments.into_iter().map(|s| s.into()).collect());
        self
    }

    /// Set the environment variables
    #[allow(dead_code)]
    pub fn environment_variables<I, K, V>(mut self, environment_variables: I) -> LaunchdPlist
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.environment_variables = Some(
            environment_variables
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        );
        self
    }

    /// Insert an environment variable
    #[allow(dead_code)]
    pub fn environment_variable<K, V>(mut self, key: K, value: V) -> LaunchdPlist
    where
        K: Into<String>,
        V: Into<String>,
    {
        match self.environment_variables {
            Some(ref mut env) => {
                env.insert(key.into(), value.into());
            },
            None => {
                self.environment_variables = Some(BTreeMap::from_iter(vec![(key.into(), value.into())]));
            },
        };
        self
    }

    /// Set the standard in path
    #[allow(dead_code)]
    pub fn standard_in_path(mut self, path: impl Into<String>) -> LaunchdPlist {
        self.standard_in_path = Some(path.into());
        self
    }

    /// Set the standard out path
    #[allow(dead_code)]
    pub fn standard_out_path(mut self, path: impl Into<String>) -> LaunchdPlist {
        self.standard_out_path = Some(path.into());
        self
    }

    /// Set the standard error path
    #[allow(dead_code)]
    pub fn standard_error_path(mut self, path: impl Into<String>) -> LaunchdPlist {
        self.standard_error_path = Some(path.into());
        self
    }

    /// Set the working directory
    #[allow(dead_code)]
    pub fn working_directory(mut self, path: impl Into<String>) -> LaunchdPlist {
        self.working_directory = Some(path.into());
        self
    }

    /// Set whether the job should be run at load
    pub fn run_at_load(mut self, run_at_load: bool) -> LaunchdPlist {
        self.run_at_load = Some(run_at_load);
        self
    }

    /// Set whether the job should be kept alive
    pub fn keep_alive(mut self, keep_alive: bool) -> LaunchdPlist {
        self.keep_alive = Some(keep_alive);
        self
    }

    /// Set the throttle interval
    pub fn throttle_interval(mut self, interval: i64) -> LaunchdPlist {
        self.throttle_interval = Some(interval);
        self
    }

    /// Set the watch paths
    pub fn watch_paths<I, T>(mut self, watch_paths: I) -> LaunchdPlist
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.watch_paths = Some(watch_paths.into_iter().map(|s| s.into()).collect());
        self
    }

    /// Set the program arguments
    pub fn associated_bundle_identifiers<I, T>(mut self, associated_bundle_identifiers: I) -> LaunchdPlist
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.associated_bundle_identifiers =
            Some(associated_bundle_identifiers.into_iter().map(|s| s.into()).collect());
        self
    }
}

pub fn create_launch_agent(launchd_plist: &LaunchdPlist) -> Result<(), super::Error> {
    let path = launchd_plist.get_file_path()?;
    let contents = launchd_plist.plist();
    if let Some(parent_dir) = path.parent() {
        std::fs::create_dir_all(parent_dir)?;
    }
    std::fs::write(&path, contents.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::consts::APP_BUNDLE_ID;

    #[test]
    fn test_plist() {
        let plist = LaunchdPlist::new(format!("{APP_BUNDLE_ID}.test"))
            .program("hello")
            .program_arguments(["hello", "test"])
            .environment_variables([("TEST", "test"), ("TEST2", "test2")])
            .standard_in_path("/dev/null")
            .standard_out_path("/dev/null")
            .standard_error_path("/dev/null")
            .run_at_load(true)
            .keep_alive(false)
            .throttle_interval(10)
            .associated_bundle_identifiers([APP_BUNDLE_ID])
            .plist();

        println!("{plist}");

        let valid_plist = indoc::formatdoc! {"
            <?xml version=\"1.0\" encoding=\"UTF-8\"?>
            <!DOCTYPE plist PUBLIC \"-//Apple Computer//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
            <plist version=\"1.0\">
                <dict>
                    <key>Label</key>
                    <string>{APP_BUNDLE_ID}.test</string>
                    <key>Program</key>
                    <string>hello</string>
                    <key>ProgramArguments</key>
                    <array>
                        <string>hello</string>
                        <string>test</string>
                    </array>
                    <key>EnvironmentVariables</key>
                    <dict>
                        <key>TEST</key>
                        <string>test</string>
                        <key>TEST2</key>
                        <string>test2</string>
                    </dict>
                    <key>StandardInPath</key>
                    <string>/dev/null</string>
                    <key>StandardOutPath</key>
                    <string>/dev/null</string>
                    <key>StandardErrorPath</key>
                    <string>/dev/null</string>
                    <key>RunAtLoad</key>
                    <true/>
                    <key>KeepAlive</key>
                    <false/>
                    <key>ThrottleInterval</key>
                    <integer>10</integer>
                    <key>AssociatedBundleIdentifiers</key>
                    <array>
                        <string>{APP_BUNDLE_ID}</string>
                    </array>
                </dict>
            </plist>
        "};

        assert_eq!(plist, valid_plist);
    }
}

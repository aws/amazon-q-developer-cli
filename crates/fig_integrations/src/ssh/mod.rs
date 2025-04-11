use std::fs::{
    DirBuilder,
    File,
};
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;

use async_trait::async_trait;
use fig_util::PRODUCT_NAME;
use fig_util::consts::CLI_BINARY_NAME;
use fig_util::directories::{
    self,
    fig_data_dir_utf8,
    home_dir,
};
use regex::Regex;

use crate::error::{
    Error,
    Result,
};
use crate::{
    FileIntegration,
    Integration,
    backup_file,
};

const SSH_CONFIG_PATH: &[&str] = &[".ssh", "config"];

const SSH_OUTER_NAME: &str = "ssh";
const SSH_INNER_NAME: &str = "ssh_inner";

#[derive(Debug, Clone)]
pub struct SshIntegration {
    path: PathBuf,
}

impl SshIntegration {
    pub fn new() -> Result<Self, Error> {
        let mut path = home_dir()?;
        path.extend(SSH_CONFIG_PATH);
        Ok(SshIntegration { path })
    }

    #[allow(clippy::unused_self)]
    fn get_integration_path(&self) -> Result<PathBuf> {
        Ok(directories::fig_data_dir()?.join(SSH_OUTER_NAME))
    }

    fn get_file_integration(&self) -> Result<FileIntegration> {
        let bin_name = CLI_BINARY_NAME;
        let include_path = fig_data_dir_utf8()?.join(SSH_INNER_NAME);

        Ok(FileIntegration {
            path: self.get_integration_path()?,
            contents: indoc::formatdoc! {"
                Match all exec=\"command -v {bin_name} && {bin_name} internal generate-ssh --remote-host %h --remote-port %p --remote-username %r\"
                    Include \"{include_path}\"
            "},
            #[cfg(unix)]
            mode: Some(0o600),
        })
    }

    fn path_text(&self) -> Result<String> {
        let home = home_dir()?;
        let integration_path = self.get_integration_path()?;
        let path = integration_path.strip_prefix(home)?;
        Ok(format!(r#""~/{}""#, path.display()))
    }

    #[allow(clippy::unused_self)]
    fn description_text(&self) -> String {
        format!("# {PRODUCT_NAME} SSH Integration. Keep at the bottom of this file.")
    }

    fn match_text(&self) -> Result<String> {
        Ok(format!("Match all\n  Include {}", self.path_text()?))
    }

    fn source_text(&self) -> Result<String> {
        let description = self.description_text();
        let match_text = self.match_text()?;
        Ok(format!("{description}\n{match_text}\n"))
    }

    fn match_text_regex(&self) -> Result<String> {
        Ok(format!(
            r#"Match all\n\s+Include\s+{}"#,
            regex::escape(&self.path_text()?)
        ))
    }

    fn source_regex(&self) -> Result<Regex> {
        let regex = format!(
            r#"(?:{}\n)?{}\n{{0,2}}"#,
            regex::escape(&self.description_text()),
            &self.match_text_regex()?
        );
        Ok(Regex::new(&regex)?)
    }

    fn check_regex_is_match(&self, contents: &str) -> Result<()> {
        let filtered_contents = Regex::new(r"^\s*(#.*)?\n").unwrap().replace_all(contents, "");
        if !self.source_regex()?.is_match(&filtered_contents) {
            return Err(Error::NotInstalled(
                format!("{:?} does not source {PRODUCT_NAME}'s ssh integration", self.path).into(),
            ));
        }
        Ok(())
    }

    pub async fn reinstall(&self) -> Result<()> {
        if self.get_integration_path()?.exists() {
            self.get_file_integration()?.install().await?;
        }
        Ok(())
    }

    /// Uninstall `~/.ssh/config` integrations
    async fn uninstall_ssh_config(&self) -> Result<()> {
        if self.path.exists() {
            let mut contents = std::fs::read_to_string(&self.path)?;
            contents = self.source_regex()?.replace_all(&contents, "").into();
            contents = contents.trim().to_string();
            contents.push('\n');
            std::fs::write(&self.path, contents.as_bytes())?;
        }
        Ok(())
    }
}

#[async_trait]
impl Integration for SshIntegration {
    fn describe(&self) -> String {
        "SSH Integration".to_owned()
    }

    async fn install(&self) -> Result<()> {
        // Always update the file integration, these are not user facing
        self.get_file_integration()?.install().await?;

        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        // Create the .ssh directory if it doesn't exist
        if let Some(path) = self.path.parent() {
            if !path.exists() {
                let mut builder = DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                builder.mode(0o700);
                builder.create(path)?;
            }
        }

        let mut contents = if self.path.exists() {
            backup_file(&self.path, fig_util::directories::utc_backup_dir().ok())?;
            self.uninstall_ssh_config().await?;
            std::fs::read_to_string(&self.path)?
        } else {
            String::new()
        };

        let source_text = self.source_text()?;

        if !contents.is_empty() {
            contents.push('\n');
        }
        contents.push_str(&source_text);

        let mut file = File::create(&self.path)?;
        file.write_all(contents.as_bytes())?;

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        let file_integration = self.get_file_integration()?;
        let (res_1, res_2, res_3) = tokio::join!(self.uninstall_ssh_config(), file_integration.uninstall(), async {
            // delete inner ssh integration file, ignore the error
            let _ = tokio::fs::remove_file(directories::fig_data_dir()?.join(SSH_INNER_NAME)).await;
            Ok(())
        });

        res_1.and(res_2).and(res_3)
    }

    async fn is_installed(&self) -> Result<()> {
        self.get_file_integration()?.is_installed().await?;

        let contents = match std::fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::NotInstalled(
                    format!("{:?} does not exist: {err}", self.path).into(),
                ));
            },
            Err(err) => {
                return Err(Error::NotInstalled(
                    format!("Error reading {:?}: {err}", self.path).into(),
                ));
            },
        };

        self.check_regex_is_match(&contents)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_integration() {
        let integration = SshIntegration::new().unwrap();
        let file_integration = integration.get_file_integration().unwrap();

        println!("=== source_text ===");
        println!("{}", integration.match_text().unwrap());
        println!("===================");
        println!();
        println!("=== file_integration ===");
        println!("{}", file_integration.contents);
        println!("========================");
    }

    #[test]
    fn test_integration_regex() {
        let integration = SshIntegration::new().unwrap();
        let re = integration.source_regex().unwrap();
        println!("{}", re.as_str());

        // Replaces whole integration
        let base = integration.source_text().unwrap();
        assert!(integration.check_regex_is_match(&base).is_ok());
        assert_eq!(re.replace_all(&base, ""), String::new());

        // A more complex example
        let path = integration.path_text().unwrap();
        let description = integration.description_text();
        let match_text = integration.match_text().unwrap();
        let config_text = indoc::formatdoc! {"
            {description}
            {match_text}
               
            # Non Match
            Match all 1
                Include \"~/path/to/file\"

            # Match 1
            Match all
                Include {path}

            {match_text}

            # Match 2
            Match all
            \t\tInclude {path}
            # Match 3
            Match all
             Include   {path}
            {description}
            {match_text}
            {match_text}
            # Non match 2
            {description}
            Match all
                Include \"~/path/to/file\"
            {description}
            {match_text}
        "};

        assert!(integration.check_regex_is_match(&config_text).is_ok());

        // replace whole integration
        let replaced_config_text = re.replace_all(&config_text, "");
        println!("=== replaced_config_text ===");
        println!("{replaced_config_text}");
        println!("============================");

        assert!(integration.check_regex_is_match(&replaced_config_text).is_err());

        // count the number of "all" to ensure match is replaced
        let all_re = Regex::new(r"Match all").unwrap();
        let all_count = all_re.find_iter(&replaced_config_text).count();
        assert_eq!(all_count, 2);

        // count the number of "Amazon" to ensure match is replaced
        let amazon_re = Regex::new(r"# Amazon").unwrap();
        let amazon_count = amazon_re.find_iter(&replaced_config_text).count();
        assert_eq!(amazon_count, 1);
    }
}

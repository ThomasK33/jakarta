use async_trait::async_trait;

pub struct ShCommand {}

#[async_trait]
impl jakarta::JakartaCommand for ShCommand {
    async fn process(
        &mut self,
        _command: String,
        args: String,
        default_value: Option<String>,
    ) -> String {
        let cmd = std::process::Command::new("sh")
            .arg("-c")
            .arg(args.clone())
            .output();

        match cmd {
            Ok(cmd) => String::from_utf8(cmd.stdout).unwrap_or_else(|_| {
                tracing::warn!(
                    "Could not obtain stdout from process {args:?}, resolving to default value"
                );

                default_value.unwrap_or_else(|| "".to_owned())
            }),
            Err(err) => {
                tracing::warn!("Failed to execute process {args:?}: {err}");

                default_value.unwrap_or_else(|| "".to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use jakarta::{Jakarta, JakartaCommand};
    use tokio::sync::Mutex;

    use super::*;

    #[tokio::test]
    async fn it_runs_shell_commands() {
        let mut commands: HashMap<&str, Arc<Mutex<dyn JakartaCommand>>> = HashMap::new();

        let sh_cmd = Arc::new(Mutex::new(ShCommand {}));
        commands.insert("sh", sh_cmd.clone());
        let jakarta = Jakarta::new(commands).unwrap();

        let result = jakarta
            .interpolate_string("asd ${sh:printf 1}".to_owned())
            .await;

        assert_eq!(result, "asd 1".to_owned());
    }
}

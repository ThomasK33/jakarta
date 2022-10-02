use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;

use regex::Regex;
use thiserror::Error;

use crate::commands::JakartaCommand;

#[derive(Error, Debug)]
pub enum JakartaError {
    #[error("failed to compile regex")]
    RegexCompilation(#[from] regex::Error),
}

pub struct Jakarta<'a> {
    interpolation_regex: Regex,
    command_map: HashMap<&'a str, Arc<Mutex<dyn JakartaCommand>>>,
}

impl<'a> Jakarta<'a> {
    pub fn new(
        command_map: HashMap<&'a str, Arc<Mutex<dyn JakartaCommand>>>,
    ) -> Result<Self, JakartaError> {
        Ok(Self {
            interpolation_regex: Regex::new(
                r"\$\{(?:\s*(?P<command>[a-zA-Z0-9-_]+)\s*:\s*(?P<path>[^{}]+?)\s*(?:(#|\?)(?P<field>[^{}]*?)){0,1}?(?:(:)(?P<default_value>.+)){0,1}\s*?){0,1}}",
            )?,
            command_map,
        })
    }

    pub async fn interpolate_string(&self, original: String) -> String {
        let mut interpolated_string = original;

        while self.interpolation_regex.is_match(&interpolated_string) {
            interpolated_string = self.replace_values(&interpolated_string).await;
        }

        interpolated_string
    }

    async fn replace_values(&self, interpolated_string: &str) -> String {
        let mut resulting_string = interpolated_string.to_owned();

        for value in self.interpolation_regex.captures_iter(interpolated_string) {
            let matched_full_string = match value.get(0) {
                Some(value) => value.as_str(),
                None => {
                    continue;
                }
            };

            let value = if let Some(command) = value.name("command") {
                if let Some(path) = value.name("path") {
                    let command_id = command.as_str();
                    let path = path.as_str();
                    let field = value.name("field").map(|field| field.as_str());
                    let default_value = value
                        .name("default_value")
                        .map(|default_value| default_value.as_str());

                    if let Some(command) = self.command_map.get(command_id) {
                        command
                            .lock()
                            .await
                            .process(
                                command_id.to_owned(),
                                path.to_owned(),
                                field.map(|f| f.to_owned()),
                                default_value.map(|dv| dv.to_owned()),
                            )
                            .await
                    } else {
                        "".to_owned()
                    }
                } else {
                    "".to_owned()
                }
            } else {
                "".to_owned()
            };

            resulting_string = resulting_string.replace(matched_full_string, value.as_str());
        }

        resulting_string
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_instantiates_new() {
        let _ = Jakarta::new(HashMap::new());
    }

    #[test]
    fn it_registers_commands() {
        use async_trait::async_trait;

        struct TestCommand {}

        #[async_trait]
        impl JakartaCommand for TestCommand {
            async fn process(
                &mut self,
                _command: String,
                _path: String,
                _field: Option<String>,
                _default_value: Option<String>,
            ) -> String {
                todo!()
            }
        }

        let mut commands: HashMap<&str, Arc<Mutex<dyn JakartaCommand>>> = HashMap::new();
        let test_cmd = Arc::new(Mutex::new(TestCommand {}));
        commands.insert("test", test_cmd);

        let _ = Jakarta::new(commands).unwrap();
    }

    #[tokio::test]
    async fn it_interpolates_with_no_commands() {
        let jakarta = Jakarta::new(HashMap::new()).unwrap();
        let result = jakarta
            .interpolate_string("asd ${env:TEST}".to_owned())
            .await;

        assert_eq!(result, "asd ".to_owned());
    }

    #[tokio::test]
    async fn it_registers_interpolates_using_command() {
        use async_trait::async_trait;

        struct TestCommand {
            counter: u8,
        }

        #[async_trait]
        impl JakartaCommand for TestCommand {
            async fn process(
                &mut self,
                command: String,
                path: String,
                _field: Option<String>,
                default_value: Option<String>,
            ) -> String {
                self.counter += 1;

                if command == "test" {
                    path
                } else if command == "test_2" {
                    default_value.unwrap_or("default".to_owned())
                } else {
                    "".to_owned()
                }
            }
        }

        let mut commands: HashMap<&str, Arc<Mutex<dyn JakartaCommand>>> = HashMap::new();

        let test_cmd = Arc::new(Mutex::new(TestCommand { counter: 0 }));
        commands.insert("test", test_cmd.clone());
        commands.insert("test_2", test_cmd.clone());
        let jakarta = Jakarta::new(commands).unwrap();

        let result = jakarta
            .interpolate_string("asd ${test:123}".to_owned())
            .await;

        assert_eq!(result, "asd 123".to_owned());

        let result = jakarta
            .interpolate_string("asd ${test:123} ${test_2:123}".to_owned())
            .await;
        assert_eq!(result, "asd 123 default".to_owned());

        let result = jakarta
            .interpolate_string("asd ${test:123} ${test_2:123:my default value}".to_owned())
            .await;
        assert_eq!(result, "asd 123 my default value".to_owned());

        assert_eq!(test_cmd.lock().await.counter, 5);
    }
}

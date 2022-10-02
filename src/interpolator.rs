use std::{collections::HashMap, str::FromStr};

use regex::Regex;
// use serde_json::Value;

#[derive(Debug)]
enum Entry {
    // Kv1(anyhow::Result<serde_json::Map<String, Value>>),
    // Kv2(anyhow::Result<serde_json::Map<String, Value>>),
    Db(Option<(String, String)>),
    Env(Option<String>),
    Sh(Option<String>),
}

pub(crate) async fn interpolate_string(
    original_string: String,
    vault_addr: &str,
    vault_token: String,
) -> String {
    // Example: ${kv1:secrets/secret/name#field:default_value}
    let interpolation_regex = Regex::new(
          r"\$\{(?:\s*(?P<type>[a-zA-Z0-9]+)\s*:\s*(?P<path>[^{}]+?)\s*(?:(#|\?)(?P<field>[^{}]*?)){0,1}?(?:(:)(?P<default_value>.+)){0,1}\s*?){0,1}}",
      )
      .unwrap();

    let mut interpolated_string = original_string;

    // Mapping type, path and entries
    let mut interpolations: HashMap<String, HashMap<String, Option<Entry>>> = HashMap::new();

    while interpolation_regex.is_match(&interpolated_string) {
        interpolated_string = replace_values(
            &interpolation_regex,
            &vault_client,
            &mut interpolations,
            &interpolated_string,
        )
        .await;
    }

    interpolated_string
}

async fn replace_values(
    interpolation_regex: &Regex,
    vault_client: &vault::Client,
    interpolations: &mut HashMap<String, HashMap<String, Option<Entry>>>,
    original_str: &str,
) -> String {
    let mut resulting_string = original_str.to_owned();

    for value in interpolation_regex.captures_iter(original_str) {
        let matched_full_string = match value.get(0) {
            Some(value) => value.as_str(),
            None => {
                continue;
            }
        };

        if let Some(type_name) = value.name("type") {
            if let Some(path) = value.name("path") {
                let field = match value.name("field") {
                    Some(val) => val.as_str(),
                    None => {
                        if type_name.as_str() != "env" && type_name.as_str() != "sh" {
                            tracing::error!(
                                "Missing field name in \"{}\"; omitting interpolation",
                                matched_full_string
                            );
                        }

                        ""
                    }
                };

                let type_name = type_name.as_str();
                let path = path.as_str();

                if !interpolations.contains_key(type_name) {
                    interpolations.insert(type_name.to_owned(), HashMap::new());
                }

                let interpolation_path_map = match interpolations.get_mut(type_name) {
                    Some(map) => map,
                    None => continue,
                };

                if !interpolation_path_map.contains_key(path) {
                    interpolation_path_map.insert(path.to_owned(), None);
                }

                let interpolation_entry = match interpolation_path_map.get_mut(path) {
                    Some(map) => map,
                    None => continue,
                };

                let default_value = value
                    .name("default_value")
                    .map(|default_value| default_value.as_str());

                let entry: Option<&Entry> = match &interpolation_entry {
                    Some(entry) => Some(entry),
                    None => {
                        // Fetch entry
                        match EntryType::from_str(type_name) {
                            Ok(entry_type) => {
                                let entry =
                                    fetch_entry(vault_client, entry_type, path, default_value)
                                        .await;

                                interpolation_path_map.insert(path.to_owned(), Some(entry));

                                Some(interpolation_path_map.get(path).unwrap().as_ref().unwrap())
                            }
                            Err(err) => {
                                tracing::error!("{}", err);

                                None
                            }
                        }
                    }
                };

                // Interpoalte string
                let value: &str = if let Some(entry) = entry {
                    match entry {
                        Entry::Kv1(Ok(kv1)) => kv1
                            .get(field)
                            .and_then(|val| val.as_str())
                            .or_else(|| default_value)
                            .unwrap_or(""),
                        Entry::Kv2(Ok(kv2)) => kv2
                            .get(field)
                            .and_then(|val| val.as_str())
                            .or_else(|| default_value)
                            .unwrap_or(""),
                        Entry::Db(Some((username, password))) => {
                            if field == "username" {
                                username.as_str()
                            } else if field == "password" {
                                password.as_str()
                            } else {
                                default_value.unwrap_or("")
                            }
                        }
                        Entry::Env(Some(env)) => env.as_str(),
                        Entry::Env(None) => default_value.unwrap_or(""),
                        Entry::Sh(Some(sh)) => sh.as_str(),
                        Entry::Sh(None) => default_value.unwrap_or(""),
                        _ => "",
                    }
                } else {
                    ""
                };

                resulting_string = resulting_string.replace(matched_full_string, value);
            }
        }
    }

    resulting_string
}

#[derive(Debug)]
enum EntryType {
    Kv1,
    Kv2,
    Db,
    Env,
    Sh,
}

impl FromStr for EntryType {
    type Err = anyhow::Error;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        if str == "kv1" {
            Ok(Self::Kv1)
        } else if str == "kv2" {
            Ok(Self::Kv2)
        } else if str == "db" {
            Ok(Self::Db)
        } else if str == "env" {
            Ok(Self::Env)
        } else if str == "sh" {
            Ok(Self::Sh)
        } else {
            anyhow::bail!("Could not match EntryType from: {}", str)
        }
    }
}

async fn fetch_entry(
    vault_client: &vault::Client,
    entry_type: EntryType,
    secret_path: &str,
    default_value: Option<&str>,
) -> Entry {
    match entry_type {
        // Fetch kv1 secrets
        EntryType::Kv1 => Entry::Kv1({
            match vault_client
                .read_kv1_secret(secret_path)
                .await
                .map_err(anyhow::Error::msg)
            {
                Ok(vault::responses::ResponseWrapper::Response(res)) => {
                    Ok(res.data.unwrap_or_default())
                }
                Ok(vault::responses::ResponseWrapper::Error { errors }) => Err(anyhow::Error::msg(
                    format!("Failed to fetch Kv1 secrets from vault: {errors:?}"),
                )),
                Err(err) => Err(anyhow::Error::msg(format!(
                    "Failed to perform Kv1 secrets request: {err}"
                ))),
            }
        }),
        // Fetch kv2 secret
        EntryType::Kv2 => Entry::Kv2({
            match vault_client
                .read_kv2_secret(secret_path, None)
                .await
                .map_err(anyhow::Error::msg)
            {
                Ok(vault::responses::ResponseWrapper::Response(res)) => res
                    .data
                    .map(|data| data.data)
                    .ok_or_else(|| anyhow::Error::msg("Could not obtain Kv2 secret value")),
                Ok(vault::responses::ResponseWrapper::Error { errors }) => Err(anyhow::Error::msg(
                    format!("Failed to fetch Kv2 secrets from vault: {errors:?}"),
                )),
                Err(err) => Err(anyhow::Error::msg(format!(
                    "Failed to perform Kv2 secrets request: {err}"
                ))),
            }
        }),
        EntryType::Db => {
            // Fetch db secrets
            let db_credentials = match vault_client.get_database_credentials(secret_path).await {
                Ok(val) => val,
                Err(err) => {
                    tracing::error!("{:?} {}: {}", entry_type, secret_path, err);
                    return Entry::Db(None);
                }
            };

            match db_credentials {
                vault::responses::ResponseWrapper::Response(res) => {
                    Entry::Db(res.data.map(|data| (data.username, data.password)))
                }
                vault::responses::ResponseWrapper::Error { errors } => {
                    tracing::error!("Failed to obtain DB credentials: {errors:?}");
                    Entry::Db(None)
                }
            }
        }
        EntryType::Env => match std::env::var(secret_path) {
            Ok(var) => Entry::Env(Some(var)),
            Err(err) => {
                if let Some(default_value) = default_value {
                    Entry::Env(Some(default_value.to_owned()))
                } else {
                    tracing::error!("{:?} {}: {}", entry_type, secret_path, err);
                    Entry::Env(None)
                }
            }
        },
        EntryType::Sh => {
            let cmd = std::process::Command::new("sh")
                .arg("-c")
                .arg(secret_path)
                .output()
                .expect("failed to execute process");

            tracing::debug!("{}", secret_path);

            match String::from_utf8(cmd.stdout) {
                Ok(var) => Entry::Sh(Some(var)),
                Err(err) => {
                    if let Some(default_value) = default_value {
                        Entry::Sh(Some(default_value.to_owned()))
                    } else {
                        tracing::error!("{:?} {}: {}", entry_type, secret_path, err);
                        Entry::Sh(None)
                    }
                }
            }
        }
    }
}

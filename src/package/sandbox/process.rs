use std::{collections::HashMap, fs, path::Path};

use crate::{
    errors::CatError,
    package::sandbox::{generate_config, shell_cmd},
};

pub fn sandbox_exec<P, Q>(
    temp_path: P,
    script_path: Q,
) -> Result<Option<HashMap<String, String>>, CatError>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let config = generate_config(temp_path.as_ref().to_string_lossy().as_ref());
    let mut child = std::process::Command::new("sandbox-exec")
        .arg("-p")
        .arg(config)
        .arg("bash")
        .arg("-c")
        .arg(shell_cmd(
            temp_path.as_ref().to_string_lossy().as_ref(),
            script_path.as_ref().to_string_lossy().to_string().as_ref(),
        ))
        .spawn()
        .map_err(|e| CatError::Task(format!("failed to spawn sandbox-exec: {}", e)))?;
    println!("waiting for sandbox-exec finish");
    let exit_status = child.wait()?;
    if exit_status.success() {
        println!("sandbox-exec completed successfully");
        let toml_path = temp_path.as_ref().join("result.toml");
        if toml_path.exists() {
            let content = fs::read_to_string(&toml_path)?;
            let variables: HashMap<String, String> = toml::from_str(&content).map_err(|e| {
                CatError::Task(format!("Can not read variables from result.toml: {e}"))
            })?;
            return Ok(Some(variables));
        }
    } else {
        return Err(CatError::Task(format!(
            "sandbox-exec failed with exit code: {}",
            exit_status
        )));
    }
    Ok(None)
}

#[test]
fn test_sandbox() {
    use crate::macos::file::remove_dir_recursively_force;
    let temp = std::env::temp_dir().canonicalize().unwrap();
    let temp = temp.join("pac_task_temp");
    if temp.exists() {
        remove_dir_recursively_force(&temp).unwrap()
    }
    fs::create_dir_all(&temp).unwrap();
    let script_path = temp.join("get.sh");
    fs::copy("./tests/get.sh", &script_path).unwrap();
    let variables = sandbox_exec(&temp, &script_path).unwrap();
    println!("{:?}", variables.unwrap());
}

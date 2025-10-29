use std::{collections::HashMap, fs};

use crate::{
    macos::file::remove_dir_recursively_force,
    package::{sandbox::process::sandbox_exec, script::Task},
};

pub async fn handle_task(
    task: &Task,
    variables: &mut HashMap<String, String>,
) -> Result<(), crate::errors::CatError> {
    let temp = std::env::temp_dir().canonicalize()?;
    let temp_path = temp.join("pac_task_temp");
    if temp_path.exists() {
        remove_dir_recursively_force(&temp_path).unwrap()
    }
    fs::create_dir_all(&temp_path)?;
    let script_path = temp_path.join(&task.script);
    fs::copy(&task.script, &script_path)?;
    let res = sandbox_exec(temp_path, script_path)?;
    if let Some(map) = res {
        variables.extend(map);
    }
    Ok(())
}

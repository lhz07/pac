use std::{collections::HashMap, fs};

use crate::{
    macos::file::remove_dir_recursively_force,
    package::{sandbox::process::sandbox_exec, script::Task},
    scopeguard::DropGuard,
};

pub async fn handle_task(
    task: &Task,
    variables: &mut HashMap<String, String>,
    dir_prefix: &std::path::Path,
) -> Result<(), crate::errors::CatError> {
    let temp = std::env::temp_dir().canonicalize()?;
    let temp_path = temp.join("pac_task_temp");
    if temp_path.exists() {
        remove_dir_recursively_force(&temp_path)?;
    }
    fs::create_dir_all(&temp_path)?;
    let _temp_guard = DropGuard::new(&temp_path, |temp_path| {
        if let Err(e) = remove_dir_recursively_force(temp_path) {
            eprintln!(
                "Warning: can not remove temp dir or its content: {}, error: {e}",
                temp_path.display()
            )
        }
    });
    let script_path = temp_path.join(&task.script);
    fs::copy(dir_prefix.join(&task.script), &script_path)?;
    let res = sandbox_exec(&temp_path, script_path)?;
    if let Some(map) = res {
        variables.extend(map);
    }
    // TODO: copy the dirs in DIR_TO_INSTALL to the cache path
    Ok(())
}

pub mod process;

const PROFILE: &str = include_str!("./sandbox.sb");

pub fn generate_config(path: &str) -> String {
    // allow read and write to a temporary directory
    format!(r#"{}(allow file* (subpath "{}"))"#, PROFILE, path)
}

pub fn shell_cmd(temp_path: &str, script_path: &str) -> String {
    format!("cd {temp_path}\nbash {script_path}\n")
}

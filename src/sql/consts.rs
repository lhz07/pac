pub const INIT_DB: &str = include_str!("init_database.sql");

pub const INSERT_PAC: &str = include_str!("insert_pac.sql");
pub const INSERT_DEP: &str = include_str!("insert_dep.sql");
pub const INSERT_CONFLICT: &str = include_str!("insert_conflict.sql");
pub const INSERT_INSTALLED_FILE: &str = include_str!("insert_installed_file.sql");

pub const SELECT_PAC_NAME: &str = include_str!("select_pac_name.sql");
pub const SELECT_PAC_NAMES: &str = include_str!("select_pac_names.sql");
pub const SELECT_PAC_ID: &str = include_str!("select_pac_id.sql");
pub const SELECT_EXIST_FILE: &str = include_str!("select_exist_file.sql");
pub const SELECT_INSTALLED_FILE: &str = include_str!("select_installed_file.sql");
pub const SELECT_REVERSE_DEP: &str = include_str!("select_reverse_dep.sql");
pub const SELECT_ORPHAN_PAC: &str = include_str!("select_orphan_pac.sql");

pub const DELETE_PAC: &str = include_str!("delete_pac.sql");

pub const UPDATE_PAC_STATE: &str = include_str!("update_pac_state.sql");

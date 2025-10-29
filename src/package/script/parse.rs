use std::collections::{HashMap, HashSet};

use strfmt::strfmt;

use crate::{
    errors::CatError,
    package::script::{Dependency, Pac, task::handle_task},
};

pub async fn parse_script(content: String) -> Result<(), CatError> {
    let mut pac: Pac = toml::from_str(&content).unwrap();
    println!("{:?}", pac);
    let mut variable_map = HashMap::new();
    for task in pac.task.iter() {
        handle_task(&task, &mut variable_map).await?;
    }
    for file in pac.file.iter_mut() {
        let url = strfmt(&file.url, &variable_map)
            .map_err(|e| CatError::Task(format!("fmt value {} error: {}", file.url, e)))?;
        println!("{url}");
        file.url = url;
        if let Some(checksum) = &mut file.checksum {
            let checksum_str = strfmt(&checksum.value, &variable_map).map_err(|e| {
                CatError::Task(format!("fmt value {} error: {}", checksum.value, e))
            })?;
            println!("{checksum_str}");
            checksum.value = checksum_str;
        }
    }
    if let Some(update) = &mut pac.basic.self_update {
        let update_str = strfmt(update, &variable_map)
            .map_err(|e| CatError::Task(format!("fmt value {update} error: {}", e)))?;
        println!("{update_str}");
        *update = update_str;
    }
    println!("{:?}", pac);
    let deps = parse_deps(pac.basic.dependencies);
    println!("{:?}", deps);
    Ok(())
}

pub fn parse_deps(deps: Vec<String>) -> Vec<Dependency> {
    const SEP: &str = " | ";
    let mut dependency = Vec::new();
    for dep in deps {
        if dep.contains(SEP) {
            let mut set = HashSet::new();
            for d in dep.split(SEP) {
                set.insert(d.to_string());
            }
            dependency.push(Dependency::Multi(set));
        } else {
            dependency.push(Dependency::Single(dep));
        }
    }
    dependency
}

#[tokio::test]
#[ignore = "just for dev"]
async fn test_parse_script() {
    let content = std::fs::read_to_string("./tests/test.toml").unwrap();
    parse_script(content).await.unwrap();
}

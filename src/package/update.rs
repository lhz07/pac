use crate::{
    brew_api::{download_multi, get_json_api_multi},
    database::{
        basic::{SqlPool, SqlRead},
        local::{PacState, SqlTransMark},
    },
    errors::CatError,
    package::list::print_columns_vertical,
};

pub async fn update(_tx: SqlTransMark) -> Result<(), CatError> {
    let mut pool = SqlPool;
    let brew_pacs = pool
        .get_pacs_by_source(crate::database::local::PacSource::Brew)
        .await?;
    let brew_pac_names = brew_pacs
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>();
    let pac_info = get_json_api_multi(&brew_pac_names).await?;
    let mut to_update = Vec::new();
    for (pac, info) in brew_pacs.iter().zip(pac_info.iter()) {
        let info_version = info.versions.stable.as_ref().unwrap();
        let rebuild = info
            .bottle
            .as_ref()
            .unwrap()
            .stable
            .as_ref()
            .unwrap()
            .rebuild;
        if &pac.version != info_version
            || (&pac.version == info_version && pac.build_epoch < rebuild as i64)
        {
            to_update.push((pac, info));
        }
    }
    if to_update.is_empty() {
        println!("All pacs are up to date.");
        return Ok(());
    }
    let mut to_install = Vec::new();
    for (_, info) in to_update.iter() {
        for dep in info.dependencies.iter() {
            match pool.is_installed(dep).await? {
                Some((_, state)) => {
                    if let PacState::Broken = state {
                        return Err(CatError::Pac(format!(
                            "package {} is broken (required by {})\n\
                             Please uninstall it first",
                            dep, info.name
                        )));
                    }
                }
                None => to_install.push(dep),
            }
        }
    }
    let mut to_download = Vec::new();
    let to_install_info = get_json_api_multi(&to_install).await?;
    for info in to_install_info.iter() {
        to_download.push(info);
    }
    println!("new pacs:");
    print_columns_vertical(
        &to_update
            .iter()
            .map(|(p, _)| p.name.as_str())
            .collect::<Vec<_>>(),
    );
    if !to_install.is_empty() {
        println!("new dependencies:");
        print_columns_vertical(&to_install);
    }
    for (_, info) in to_update.iter() {
        to_download.push(info);
    }
    let _path = download_multi(&to_download).await?;
    // mark the package as broken before update

    Ok(())
}

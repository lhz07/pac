use crate::{
    CACHE_DIR, CLIENT_WITH_RETRY, PAC_PATH,
    brew_api::{download_multi, verify_hash},
    compress::decompress_file,
    database::local::{PacState, SqlTransaction},
    errors::{CatError, CloudError, RequestError},
    macos::{
        file::{
            CmpPath, remove_dir_force, remove_dir_recursively_force, remove_file_force, unix_cp_r,
        },
        version::ARCH_OS,
    },
    package::{
        after_install::set_proper_permissions,
        find_depend::{PacInfoRef, detect_conflicts, resolve_depend},
        install::install,
        preprocess::before_install,
        script::{Basic, Pac, PacFile, task::handle_task},
    },
    scopeguard::DropGuard,
};
use flate2::read::GzDecoder;
use reqwest::header::CONTENT_LENGTH;
use sha2::Digest;
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::BufReader,
    iter::zip,
    path::{Path, PathBuf},
};
use strfmt::strfmt;
use tokio::io::AsyncWriteExt;

pub async fn parse_script<P>(content: String, dir_prefix: P) -> Result<Pac, CatError>
where
    P: AsRef<Path>,
{
    let mut pac: Pac = toml::from_str(&content)
        .map_err(|e| CatError::Pac(format!("Can not parse pac toml, error: {e}")))?;
    let mut variable_map = HashMap::new();
    for task in pac.task.iter() {
        handle_task(&task, &mut variable_map, dir_prefix.as_ref()).await?;
    }
    for file in pac.file.iter_mut() {
        let url = strfmt(&file.url, &variable_map)
            .map_err(|e| CatError::Task(format!("fmt value {} error: {}", file.url, e)))?;
        // println!("{url}");
        file.url = url;
        if let Some(checksum) = &mut file.checksum {
            let checksum_str = strfmt(&checksum.value, &variable_map).map_err(|e| {
                CatError::Task(format!("fmt value {} error: {}", checksum.value, e))
            })?;
            // println!("{checksum_str}");
            checksum.value = checksum_str;
        }
    }
    if let Some(update) = &mut pac.basic.self_update {
        let update_str = strfmt(update, &variable_map)
            .map_err(|e| CatError::Task(format!("fmt value {update} error: {}", e)))?;
        // println!("{update_str}");
        *update = update_str;
    }
    // println!("{:?}", pac);
    Ok(pac)
}

pub async fn install_pac_from_file<P>(path: P) -> Result<(), CatError>
where
    P: AsRef<Path>,
{
    let content = fs::read_to_string(path.as_ref().join("pac.toml"))?;
    let pac = parse_script(content, path.as_ref()).await?;
    let mut tx = SqlTransaction::new().await?;
    if let Some((_, state)) = tx.is_installed(&pac.basic.name).await? {
        match state {
            PacState::Installed => {
                println!("Package {} is already installed", &pac.basic.name);
                return Ok(());
            }
            PacState::Broken => {
                return Err(CatError::Pac(format!(
                    "package {} is broken, please uninstall it first",
                    &pac.basic.name
                )));
            }
        }
    }
    // TODO: support pac deps, only handle brew deps for now
    let brew_deps = parse_deps(&pac.basic.brew_dependencies, &pac.basic.name, &mut tx).await?;
    let deps = resolve_depend(&pac.basic.name, &brew_deps).await?;
    let mut to_install = Vec::new();
    for dep in deps {
        match tx.is_installed(&dep.name).await? {
            Some((_, state)) => {
                if let PacState::Broken = state {
                    return Err(CatError::Pac(format!(
                        "package {} is broken (required by {})\n\
                        Please uninstall it first",
                        dep.name, pac.basic.name
                    )));
                }
            }
            None => to_install.push(dep),
        }
    }
    println!("detecting conflicts...");
    detect_conflicts(&to_install, &mut tx).await?;
    let p = [&pac];
    detect_conflicts(&p, &mut tx).await?;
    println!("downloading pacs...");
    let paths = download_multi(&to_install).await?;
    let mut temp_paths = DropGuard::new(Vec::<PathBuf>::new(), |temp_paths| {
        // clean temp dir
        println!("cleaning temp dirs...");
        for p in temp_paths {
            let _ = remove_dir_recursively_force(&p).inspect_err(|e| {
                eprintln!(
                    "Warning: Can not clean temp path: {}, error: {e}",
                    p.display()
                )
            });
        }
        println!("temp dirs are removed!");
    });
    let mut restore_guard = DropGuard::new(Vec::<Vec<PathBuf>>::new(), |installed_files| {
        eprintln!("encounter an error, restoring install dir");
        // also remove dirs
        let mut dirs = BTreeSet::new();
        let pac_path = Path::new(PAC_PATH);
        for paths in installed_files.iter() {
            for p in paths.iter() {
                let mut ancestors = p.ancestors();
                // skip itself
                ancestors.next();
                while let Some(parent) = ancestors.next()
                    && !dirs.contains(&CmpPath(parent))
                    && parent != pac_path
                {
                    dirs.insert(CmpPath(parent));
                }
                if let Err(e) = remove_file_force(&p)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    eprintln!(
                        "Warning: Can not remove installed file: {}, error: {e}",
                        p.display()
                    )
                }
            }
        }
        for dir in dirs {
            if let Err(e) = remove_dir_force(&*dir)
                && !(e.kind() == std::io::ErrorKind::NotFound
                    || e.kind() == std::io::ErrorKind::DirectoryNotEmpty)
            {
                eprintln!(
                    "Warning: Can not remove installed dir: {}, error: {e}",
                    dir.0.display()
                )
            }
        }
        println!("recovery finished!");
    });
    // install pacs
    for (pac, mut path) in zip(to_install, paths) {
        println!("installing {}", pac.full_name);
        println!("loading downloaded files");
        let downloaded_file = fs::File::open(&path)?;
        let gz = GzDecoder::new(BufReader::new(downloaded_file));
        let mut archive = tar::Archive::new(gz);
        path.set_extension("");
        path.set_extension("");
        let mut temp_dir = std::env::temp_dir().join(path.file_name().unwrap());
        let _ = remove_dir_recursively_force(&temp_dir);
        temp_paths.push(temp_dir.clone());
        println!("extracting...");
        archive.unpack(&temp_dir)?;
        let name_version = if pac.revision > 0 {
            format!(
                "{}/{}_{}",
                pac.name,
                pac.versions.stable.as_ref().unwrap(),
                pac.revision
            )
        } else {
            format!("{}/{}", pac.name, pac.versions.stable.as_ref().unwrap())
        };
        temp_dir.push(&name_version);
        println!("preprocessing...");
        before_install(&temp_dir, &name_version)?;
        println!("preprocess done, installing...");
        let installed_files = Vec::new();
        restore_guard.push(installed_files);
        let installed_files = restore_guard.last_mut().unwrap();
        // we should ensure the path is not conflicted before calling install.
        // implmentation is in the function below
        install(&temp_dir, installed_files, &mut tx).await?;
        let files = &pac.bottle.as_ref().unwrap().stable.as_ref().unwrap().files;
        let sha256 = if let Some(file) = files.get(ARCH_OS.as_str()) {
            &file.sha256
        } else if let Some(file) = files.get("all") {
            &file.sha256
        } else {
            unreachable!("channel is only all or {}", ARCH_OS.as_str())
        };
        tx.install_a_pac(
            &pac,
            crate::database::local::PacSource::Brew,
            pac.versions.stable.as_ref().unwrap(),
            &pac.bottle.as_ref().unwrap().stable.as_ref().unwrap(),
            sha256,
            false,
            &installed_files,
        )
        .await?;
        println!("Package {} is installed now", pac.full_name);
    }
    let target_root_path = Path::new(PAC_PATH);
    let installed_files = Vec::new();
    restore_guard.push(installed_files);
    let installed_files = restore_guard.last_mut().unwrap();
    // install pac from file
    for f in pac.file.iter() {
        // TODO: remove downloaded files after install
        let file_path = download_file(&pac.basic, f).await?;
        let to =
            CACHE_DIR.join(file_path.file_name().unwrap().to_string_lossy().to_string() + ".d");
        let _file_guard = DropGuard::new(&to, |path| {
            if let Err(e) = remove_dir_recursively_force(&path) {
                eprintln!(
                    "Warning: Can not clean temp path: {}, error: {e}",
                    path.display()
                )
            };
        });
        decompress_file(&file_path, &to)?;
        for path in f.path.iter() {
            unix_cp_r(
                to.join(&path.original),
                target_root_path.join(&path.target),
                installed_files,
                &mut tx,
            )
            .await?
        }
    }
    println!("{:?}", installed_files);
    tx.install_a_pac_pac(
        &pac,
        crate::database::local::PacSource::Pac,
        true,
        installed_files,
    )
    .await?;
    tx.commit().await?;
    // IMPORTANT: cancel the drop guard
    restore_guard.into_inner();
    let bin = target_root_path.join("bin");
    if bin.exists() {
        if let Err(e) = set_proper_permissions(bin, 0o755) {
            eprintln!("Warning: Can not set proper permissions for bin dir: {}", e);
        }
    }
    let sbin = target_root_path.join("sbin");
    if sbin.exists() {
        if let Err(e) = set_proper_permissions(sbin, 0o755) {
            eprintln!(
                "Warning: Can not set proper permissions for sbin dir: {}",
                e
            );
        }
    }
    Ok(())
}

pub async fn download_file(pac_basic: &Basic, pac_file: &PacFile) -> Result<PathBuf, CatError> {
    let download_file_name = format!(
        "{}-{}-{}",
        pac_basic.name,
        pac_basic.version,
        pac_file
            .checksum
            .as_ref()
            .map(|c| c.value.to_string())
            .unwrap_or_default()
    );
    let mut path = CACHE_DIR.clone();
    path.push(download_file_name);
    if let Some(checksum) = &pac_file.checksum {
        match &checksum.method {
            crate::brew_api::HashMethod::Sha256 => {
                if let Ok(true) = verify_hash(&path, &checksum.value, sha2::Sha256::new()) {
                    println!("{} is already downloaded", path.display());
                    return Ok(path);
                }
            }
            crate::brew_api::HashMethod::Sha512 => {
                if let Ok(true) = verify_hash(&path, &checksum.value, sha2::Sha512::new()) {
                    println!("{} is already downloaded", path.display());
                    return Ok(path);
                }
            }
        }
    }
    if path.exists() {
        let _ = remove_file_force(&path);
    }
    let mut response = CLIENT_WITH_RETRY.get(&pac_file.url).send().await?;
    if !response.status().is_success() {
        return Err(CatError::Cloud(CloudError::Request(RequestError::Status(
            format!("code {}", response.status()),
        ))));
    }
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|l| l.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(CloudError::api("Can not download"))?;
    let mut file = tokio::fs::File::create(&path).await?;
    let progress = indicatif::ProgressBar::new(content_length);
    progress.set_prefix(pac_basic.name.to_string());
    // let progress = progress.with_finish(indicatif::ProgressFinish::Abandon);
    while let Some(bytes) = response.chunk().await? {
        file.write_all(&bytes).await?;
        progress.inc(bytes.len() as u64);
    }
    if let Some(checksum) = &pac_file.checksum {
        match &checksum.method {
            crate::brew_api::HashMethod::Sha256 => {
                if let Ok(true) = verify_hash(&path, &checksum.value, sha2::Sha256::new()) {
                    println!("{} is downloaded", path.display());
                    return Ok(path);
                } else {
                    return Err(CatError::Cloud(CloudError::api(
                        "Checksum verification failed",
                    )));
                }
            }
            crate::brew_api::HashMethod::Sha512 => {
                if let Ok(true) = verify_hash(&path, &checksum.value, sha2::Sha512::new()) {
                    println!("{} is downloaded", path.display());
                    return Ok(path);
                } else {
                    return Err(CatError::Cloud(CloudError::api(
                        "Checksum verification failed",
                    )));
                }
            }
        }
    }
    Ok(path)
}

pub async fn parse_deps(
    deps: &Vec<String>,
    pac_name: &str,
    tx: &mut SqlTransaction,
) -> Result<Vec<String>, CatError> {
    const SEP: &str = " | ";
    let mut dependency = Vec::new();
    for dep in deps {
        if dep.contains(SEP) {
            let mut list = Vec::new();
            for (i, d) in dep.split(SEP).enumerate() {
                if tx.is_installed(d).await?.is_some() {
                    continue;
                }
                println!("{i}. {d}");
                list.push(d);
            }
            let mut input = String::new();
            let mut index = 0;
            println!(
                "{} depends on a selectable dependency, please select one of them: (default: 0)",
                pac_name
            );
            loop {
                std::io::stdin().read_line(&mut input)?;
                if input.trim().is_empty() {
                    break;
                } else if let Ok(i) = input.trim().parse::<usize>()
                    && (0..list.len()).contains(&i)
                {
                    index = i;
                    break;
                } else {
                    println!("Please enter a valid index");
                    input.clear();
                }
            }
            dependency.push(list[index].to_string());
        } else {
            dependency.push(dep.to_string());
        }
    }
    Ok(dependency)
}

#[tokio::test]
#[ignore = "just for dev"]
async fn test_parse_script() {
    let content = std::fs::read_to_string("./tests/pac.toml").unwrap();
    parse_script(content, "./tests").await.unwrap();
}

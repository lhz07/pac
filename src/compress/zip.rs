use std::{
    fs::{self, File},
    io::BufReader,
    path::Path,
};
use zip::ZipArchive;

pub fn unzip<P, Q>(src_zip: P, dest_dir: Q) -> zip::result::ZipResult<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let src_zip = src_zip.as_ref();
    let dest_dir = dest_dir.as_ref();

    fs::create_dir_all(dest_dir).map_err(zip::result::ZipError::Io)?;

    // read zip file
    let file = File::open(src_zip).map_err(zip::result::ZipError::Io)?;
    let reader = BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        // avoid Zip Slip
        let Some(safe_path) = entry.enclosed_name() else {
            return Err(zip::result::ZipError::FileNotFound);
        };

        let out_path = dest_dir.join(safe_path);

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(zip::result::ZipError::Io)?;
        } else {
            if let Some(parent) = out_path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent).map_err(zip::result::ZipError::Io)?;
            }
            let mut outfile = File::create(&out_path).map_err(zip::result::ZipError::Io)?;
            std::io::copy(&mut entry, &mut outfile).map_err(zip::result::ZipError::Io)?;
        }
    }

    Ok(())
}

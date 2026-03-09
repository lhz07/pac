SELECT id, name, version, build_epoch
FROM installed_packages
WHERE install_source = $1;

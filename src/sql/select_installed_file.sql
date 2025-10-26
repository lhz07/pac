SELECT installed_files.path
FROM installed_files
WHERE installed_id = $1

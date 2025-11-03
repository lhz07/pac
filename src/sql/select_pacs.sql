SELECT name
FROM installed_packages
WHERE explicit = $1
ORDER BY name COLLATE NOCASE ASC;

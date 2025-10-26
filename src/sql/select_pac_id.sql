SELECT id, state
FROM installed_packages
WHERE name = $1
  AND install_root = $2
LIMIT 1;

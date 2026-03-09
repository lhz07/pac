SELECT installed_packages.name
FROM dependencies
INNER JOIN installed_packages
ON installed_packages.id = dependent_id
WHERE dep_name = $1 AND dep_type = 'runtime';

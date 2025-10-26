SELECT dependent_id
FROM dependencies
WHERE dep_name = $1 AND dep_type = 'runtime';

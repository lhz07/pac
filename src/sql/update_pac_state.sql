UPDATE installed_packages
SET state = $1
WHERE
    id = $2;

SELECT a.id, a.name, a.state
FROM installed_packages AS a
LEFT JOIN dependencies AS b
    ON a.name = b.dep_name
WHERE a.explicit = 0 AND b.dep_name IS NULL;

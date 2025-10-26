SELECT EXISTS(
    SELECT 1 FROM installed_files WHERE LOWER(path) = LOWER($1)
);

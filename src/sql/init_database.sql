PRAGMA foreign_keys = ON;

CREATE TABLE installed_packages (
  id                INTEGER PRIMARY KEY,
  name              TEXT    NOT NULL,                      -- package name
  version           TEXT    NOT NULL,                      -- version str
  build_epoch       INTEGER NOT NULL DEFAULT 0,            -- build epoch
  arch              TEXT    NOT NULL,                      -- x86_64/arm64/any
  channel           TEXT    NOT NULL,                      -- stable/beta/local
  install_root      TEXT    NOT NULL,                      -- root of install path
  explicit          INTEGER NOT NULL,                      -- 1=explict，0=install as dependency
  pinned            INTEGER NOT NULL DEFAULT 0,            -- 1=fix version, 0=auto update
  install_time      INTEGER NOT NULL,                      -- UNIX timestamp
  update_time       INTEGER NOT NULL,                      -- UNIX timestamp
  checksum          TEXT    NOT NULL,                      -- sha256 checksum of the package archive
  state             INTEGER NOT NULL DEFAULT 0,            -- 0=installed, 1=broken
  size_installed    INTEGER,                               -- size after installation
  summary           TEXT,
  homepage          TEXT,
  license           TEXT,
  UNIQUE (name, install_root)
);
CREATE INDEX idx_installed_name ON installed_packages(name);
CREATE INDEX idx_installed_exact ON installed_packages(name, install_root);
CREATE INDEX idx_installed_explicit ON installed_packages(explicit);


CREATE TABLE dependencies (
  id                     INTEGER PRIMARY KEY,
  dependent_id           INTEGER NOT NULL,     -- installed_packages.id of the dependent package
  dep_name               TEXT    NOT NULL,     -- dependency package name
  constraint_expr        TEXT,                 -- version constraint expression
  dep_type               TEXT    NOT NULL DEFAULT "runtime",     -- runtime|build|test
  optional               INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY(dependent_id)         REFERENCES installed_packages(id) ON DELETE CASCADE
);
CREATE INDEX idx_deps_by_dependent ON dependencies(dependent_id);
-- for reverse lookup of which packages depend on a given package
CREATE INDEX idx_deps_by_name ON dependencies(dep_name);

CREATE TABLE conflicts (
  id              INTEGER PRIMARY KEY,
  installed_id    INTEGER NOT NULL,  -- installed_packages.id of the package declaring the conflict
  target_name     TEXT    NOT NULL,  -- conflicting package name
  constraint_expr TEXT,              -- version constraint expression
  reason          TEXT,              -- conflict reason
  FOREIGN KEY(installed_id) REFERENCES installed_packages(id) ON DELETE CASCADE
);
CREATE INDEX idx_conflicts_target ON conflicts(target_name);

CREATE TABLE installed_files (
  id             INTEGER PRIMARY KEY,
  installed_id   INTEGER NOT NULL,    -- installed_packages.id
  path           TEXT    NOT NULL,    -- absolute path
  size           INTEGER,
  mode           INTEGER,             -- permission bits, such as 755
  uid            INTEGER,
  gid            INTEGER,
  mtime          INTEGER,
  FOREIGN KEY(installed_id) REFERENCES installed_packages(id) ON DELETE CASCADE
);

-- detect file conflicts across different packages
-- APFS is case-insensitive by default, so we use LOWER(path) for uniqueness
CREATE UNIQUE INDEX idx_files_unique_path_global ON installed_files(LOWER(path));
CREATE INDEX idx_files_by_pkg ON installed_files(installed_id);

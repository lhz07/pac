use std::{
    collections::{HashMap, HashSet},
    iter::zip,
    rc::Rc,
};


use crate::{
    brew_api::{PacInfo, get_json_api, get_json_api_multi},
    database::basic::SqlRead,
    errors::CatError,
    package::script::Pac,
};

// In the future we’ll switch to a database, so dependency parsing and database updates
// will become two separate operations — updating the database won’t always be required.
/// returned pacs not include the root pac
pub async fn resolve_depend(
    root_name: &str,
    root_deps: &Vec<String>,
) -> Result<Vec<Rc<PacInfo>>, CatError> {
    let mut cache: HashMap<Rc<String>, Rc<PacInfo>> = HashMap::new();
    let root_name = Rc::new(root_name.to_string());

    // Permanently marked nodes: already sorted and stored in the result
    let mut perm: HashSet<Rc<String>> = HashSet::new();
    // Temporarily marked nodes: nodes on the current DFS path (used for cycle detection)
    let mut temp: HashSet<Rc<String>> = HashSet::new();

    // Stack to track the current DFS path (used only for error reporting)
    let mut path_stack: Vec<Rc<String>> = Vec::new();

    // Result in topological order: dependencies come before dependents
    let mut out: Vec<Rc<PacInfo>> = Vec::new();

    // state machine frames
    enum Frame {
        Enter(Rc<String>),
        Exit(Rc<String>),
    }

    let mut stack: Vec<Frame> = vec![Frame::Enter(root_name.clone())];

    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter(name) => {
                if perm.contains(&name) {
                    continue;
                }
                if !temp.insert(name.clone()) {
                    // Node already in temp => a cycle is detected
                    let mut cycle = path_stack.clone();
                    cycle.push(name.clone());
                    eprintln!("recursive dependency!");
                    return Err(CatError::Hash("123".to_string()));
                }
                path_stack.push(name.clone());

                // Ensure node data is available (fetch if not cached)
                let deps = if !cache.contains_key(&name) {
                    if name.as_str() == root_name.as_str() {
                        root_deps.clone()
                    } else {
                        let pac = get_json_api(&name).await?;
                        cache.insert(name.clone(), Rc::new(pac));
                        cache.get(&name).unwrap().dependencies.clone()
                    }
                } else {
                    cache.get(&name).unwrap().dependencies.clone()
                };
                let deps_uncached = deps
                    .iter()
                    .filter(|s| !cache.contains_key(*s))
                    .collect::<Vec<_>>();
                let caches = get_json_api_multi(&deps_uncached).await?;
                for (name, pac) in zip(deps_uncached.iter(), caches.into_iter()) {
                    cache.insert(Rc::new(name.to_string()), Rc::new(pac));
                }
                // Push Exit first, then dependencies (post-order traversal)
                stack.push(Frame::Exit(name.clone()));
                // Reverse order to keep dependency order consistent
                for dep in deps.into_iter().rev() {
                    if !perm.contains(&dep) {
                        stack.push(Frame::Enter(Rc::new(dep)));
                    }
                }
            }
            Frame::Exit(name) => {
                // Leaving node: remove from temp, add to perm, write to result
                temp.remove(&name);
                if let Some(pos) = path_stack.iter().rposition(|n| n == &name) {
                    path_stack.remove(pos);
                }
                if perm.insert(name.clone())
                    && let Some(pac) = cache.get(&name)
                {
                    let rc = Rc::clone(pac);
                    out.push(rc);
                }
            }
        }
    }

    Ok(out)
}

pub trait PacInfoRef {
    fn name(&self) -> &str;
    fn conflicts_with(&self) -> impl Iterator<Item = &String>;
}

impl AsRef<PacInfo> for PacInfo {
    fn as_ref(&self) -> &PacInfo {
        self
    }
}

impl<T: AsRef<PacInfo>> PacInfoRef for T {
    fn name(&self) -> &str {
        &self.as_ref().name
    }
    fn conflicts_with(&self) -> impl Iterator<Item = &String> {
        self.as_ref()
            .conflicts_with
            .iter()
            .chain(self.as_ref().versioned_formulae.iter())
    }
}

impl PacInfoRef for &Pac {
    fn name(&self) -> &str {
        &self.basic.name
    }
    fn conflicts_with(&self) -> impl Iterator<Item = &String> {
        self.conflicts.keys()
    }
}

/// database read-only
pub async fn detect_conflicts<P>(pacs: &[P], conn: &mut impl SqlRead) -> Result<(), CatError>
where
    P: PacInfoRef,
{
    let set = pacs.iter().map(|p| p.name()).collect::<HashSet<_>>();
    for pac in pacs {
        for conflict_pac in pac.conflicts_with() {
            if set.get(conflict_pac.as_str()).is_some() {
                return Err(CatError::Pac(format!(
                    "pac `{}` conflicts with `{}`",
                    pac.name(),
                    conflict_pac
                )));
            }
            if conn.is_installed(conflict_pac).await?.is_some() {
                return Err(CatError::Pac(format!(
                    "pac `{}` conflicts with installed pac `{}`",
                    pac.name(),
                    conflict_pac
                )));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_resolve_depend() {
    let pac = get_json_api("imagemagick").await.unwrap();
    let res = resolve_depend(&pac.name, &pac.dependencies).await.unwrap();
    for i in res {
        println!("{}", i.full_name);
    }
}

use std::{
    collections::{HashMap, HashSet},
    iter::zip,
    rc::Rc,
};

use crate::{
    brew_api::{PacInfo, get_json_api, get_json_api_multi},
    errors::CatError,
};

/// In the future we’ll switch to a database, so dependency parsing and database updates
/// will become two separate operations — updating the database won’t always be required.
pub async fn resolve_depend(root: PacInfo) -> Result<Vec<Rc<PacInfo>>, CatError> {
    let mut cache: HashMap<Rc<String>, Rc<PacInfo>> = HashMap::new();
    let root_rc = Rc::new(root);
    cache.insert(Rc::new(root_rc.name.clone()), Rc::clone(&root_rc));

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

    let mut stack: Vec<Frame> = vec![Frame::Enter(Rc::new(root_rc.name.clone()))];

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
                if !cache.contains_key(&name) {
                    let pac = get_json_api(&name).await?;
                    cache.insert(name.clone(), Rc::new(pac));
                }
                let deps = cache.get(&name).unwrap().dependencies.clone();
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
                if perm.insert(name.clone()) {
                    let rc = Rc::clone(cache.get(&name).expect("cached PacInfo must exist"));
                    out.push(rc);
                }
            }
        }
    }

    Ok(out)
}

#[tokio::test]
async fn test_resolve_depend() {
    let pac = get_json_api("imagemagick").await.unwrap();
    let res = resolve_depend(pac).await.unwrap();
    for i in res {
        println!("{}", i.full_name);
    }
}

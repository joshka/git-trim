use anyhow::{Context, Result};
use git2::{BranchType, Config, Direction, Repository};
use log::*;

use crate::config;
use crate::simple_glob::{expand_refspec, ExpansionSide};

// given refspec for a remote: refs/heads/*:refs/remotes/origin
// master -> refs/remotes/origin/master
// refs/head/master -> refs/remotes/origin/master
pub fn get_fetch_upstream(
    repo: &Repository,
    config: &Config,
    branch: &str,
) -> Result<Option<String>> {
    let remote_name = config::get_remote(config, branch)?;
    get_upstream(repo, config, &remote_name, branch)
}

fn get_upstream(
    repo: &Repository,
    config: &Config,
    remote_name: &str,
    branch: &str,
) -> Result<Option<String>> {
    let remote = repo.find_remote(remote_name)?;
    let merge: String = if let Some(merge) = config::get_merge(config, &branch)? {
        merge
    } else {
        return Ok(None);
    };
    assert!(
        merge.starts_with("refs/"),
        "'git config branch.{}.merge' should start with 'refs/'",
        branch
    );

    if let Some(expanded) = expand_refspec(&remote, &merge, Direction::Fetch, ExpansionSide::Right)?
    {
        // TODO: is this necessary?
        let exists = repo.find_reference(&expanded).is_ok();
        if exists {
            Ok(Some(expanded))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

// given refspec for a remote: refs/heads/*:refs/heads/*
// master -> refs/remotes/origin/master
// refs/head/master -> refs/remotes/origin/master
pub fn get_push_upstream(
    repo: &Repository,
    config: &Config,
    branch: &str,
) -> Result<Option<String>> {
    if let Some(RemoteBranch {
        remote_name,
        refname,
    }) = get_push_remote_branch(repo, config, branch)?
    {
        if let Some(upstream) = get_upstream(repo, config, &remote_name, &refname)? {
            return Ok(Some(upstream));
        }
    }
    Ok(None)
}

#[derive(Eq, PartialEq, Clone)]
pub struct RemoteBranch {
    pub remote_name: String,
    pub refname: String,
}

fn get_push_remote_branch(
    repo: &Repository,
    config: &Config,
    branch: &str,
) -> Result<Option<RemoteBranch>> {
    let remote_name = config::get_push_remote(config, branch)?;

    let remote = repo.find_remote(&remote_name)?;
    let reference = repo
        .find_branch(branch, BranchType::Local)?
        .into_reference();
    let refname = reference.name().context("non utf-8 refname")?;
    if let Some(remote_branch) =
        expand_refspec(&remote, refname, Direction::Push, ExpansionSide::Right)?
    {
        return Ok(Some(RemoteBranch {
            remote_name: remote_name.to_string(),
            refname: remote_branch,
        }));
    }

    let push_default = config::get(config, "push.default")
        .with_default(&String::from("simple"))
        .read()?
        .expect("has default");

    match push_default.as_str() {
        "current" => Ok(Some(RemoteBranch {
            remote_name: remote_name.to_string(),
            refname: branch.to_string(),
        })),
        "upstream" | "tracking" | "simple" | "matching" => {
            if let Some(merge) = config::get_merge(config, &branch)? {
                Ok(Some(RemoteBranch {
                    remote_name: remote_name.clone(),
                    refname: merge,
                }))
            } else {
                warn!("The current branch {} has no upstream branch.", branch);
                Ok(None)
            }
        }
        "nothing" => unimplemented!("push.default=nothing is not implemented."),
        _ => panic!("unexpected config push.default"),
    }
}

pub fn get_remote_branch_from_ref(repo: &Repository, remote_ref: &str) -> Result<RemoteBranch> {
    assert!(remote_ref.starts_with("refs/remotes/"));
    for remote_name in repo.remotes()?.iter() {
        let remote_name = remote_name.context("non-utf8 remote name")?;
        let remote = repo.find_remote(&remote_name)?;
        if let Some(expanded) =
            expand_refspec(&remote, remote_ref, Direction::Fetch, ExpansionSide::Left)?
        {
            return Ok(RemoteBranch {
                remote_name: remote.name().context("non-utf8 remote name")?.to_string(),
                refname: expanded,
            });
        }
    }
    unreachable!("matching refspec is not found");
}
use std::env;
use dotenv::dotenv;
use regex::Regex;
use git2::{Repository};
mod git_operations;

use git_operations::{get_repo_info, clone_new_repo, create_checkout_and_update_branch, commit_changes, push_to_remote, create_pull_request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    let exclude_dir = Regex::new(r"node_modules|target|\.git")?;
    let exclude_file = Regex::new(r"\.lock|\.log|\.json|\.md")?;
    dotenv().ok();
    let search_pattern = Regex::new(&env::var("SEARCH_PATTERN").expect("SEARCH_PATTERN not found in .env file"))?;
    let replace_with = env::var("REPLACE_WITH").expect("REPLACE_WITH not found in .env file");
    let pat = env::var("GITHUB_PAT").expect("GITHUB_PAT not found in .env file");
    let username = env::var("GITHUB_USERNAME").expect("GITHUB_USERNAME not found in .env file");

    let (url, path_string, branch_name, commit_message, repo) = get_repo_info()?;
    clone_new_repo(
        &url, 
        &path_string, 
        &pat, 
        &username
    )?;
    create_checkout_and_update_branch(
        &Repository::open(&path_string)?, 
        &branch_name, 
        &path_string,
        &exclude_dir, 
        &exclude_file, 
        &search_pattern, 
        &replace_with
    )?;
    commit_changes(&Repository::open(&path_string)?, &commit_message)?;
    push_to_remote(&Repository::open(&path_string)?, &pat, &branch_name)?;
    create_pull_request(
        &repo,
        &branch_name,
        "main",
        "Map migrated tag adjustements",
        "Use mig prefix for all aws map migrated tags",
        &pat
    ).await?;
    println!("url: {}, path: {}, commit_message: {}", url, branch_name, commit_message);
    
    Ok(())
}

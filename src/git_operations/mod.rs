use git2::{Cred, RemoteCallbacks, Repository, ObjectType, Signature, IndexAddOption, PushOptions};
use std::path::Path;
use std::env;
use clap::{App, Arg};
use regex::Regex;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use walkdir::WalkDir;

pub fn clone_repo(url: &str, path: &Path, pat: &str, username: &str) -> Result<(), git2::Error> {
  let mut callbacks = RemoteCallbacks::new();
  callbacks.credentials(|_url, username_from_url, _allowed_types| {
      let username = username_from_url.unwrap_or(username);
      Cred::userpass_plaintext(username, pat)
  });

  let mut fo = git2::FetchOptions::new();
  fo.remote_callbacks(callbacks);

  let mut builder = git2::build::RepoBuilder::new();
  builder.fetch_options(fo);

  builder.clone(url, path)?;
  Ok(())
}

pub fn search_and_replace_in_file(file_path: &std::path::Path, search: &Regex, replace_with: &str) -> io::Result<()> {
  let mut content = String::new();
  {
      let mut file = File::open(file_path)?;
      file.read_to_string(&mut content)?;
  }
  let new_content = search.replace_all(&content, replace_with).to_string();
  if content != new_content {
      fs::write(file_path, new_content)?;
  }
  Ok(())
}

pub fn get_repo_info() -> Result<(String, String, String, String, String), Box<dyn std::error::Error>> {
  let matches = App::new("Update repos")
      .version("0.1.0")
      .author("Vladimir Demidov <vladimir@republik.gg>")
      .about("Update all repos")
      .arg(Arg::with_name("repo")
          .short('r')
          .long("repo")
          .help("Repo name")
          .takes_value(true))
      .arg(Arg::with_name("branch")
          .short('b')
          .long("branch")
          .help("Branch name")
          .takes_value(true))
      .arg(Arg::with_name("commit_message")
          .short('m')
          .long("commit_message")
          .help("Commit message")
          .takes_value(true))
      .get_matches();
  let repo = matches.value_of("repo").expect("No repo provided");
  let branch_name = matches.value_of("branch").expect("No branch provided").to_string();
  let commit_message = matches.value_of("commit_message").expect("No commit message provided").to_string();
  let url = format!("https://github.com/republik-io/{}.git", repo);
  let path_string = format!("../repos/{}", repo);
  Ok((url, path_string, branch_name, commit_message, repo.to_string()))
}

pub fn clone_new_repo(
  url: &str, 
  path_string: &str, 
  pat: &str, 
  username: &str 
) -> Result<(), Box<dyn std::error::Error>> {
  let path = Path::new(path_string);
  match clone_repo(url, &path, pat, username) {
      Ok(_) => println!("Cloned {}", path_string),
      Err(e) => println!("Error: {}", e),
  }
  Ok(())
}

pub fn create_checkout_and_update_branch(
  repo: &Repository, 
  branch_name: &str,
  path_string: &str, 
  exclude_dir: &Regex, 
  exclude_file: &Regex, 
  search_pattern: &Regex, 
  replace_with: &str
) -> Result<(), Box<dyn std::error::Error>> {
  let head = repo.head()?;
  let commit = head.peel(ObjectType::Commit)?.into_commit().expect("Head not a commit");
  let path = Path::new(path_string);
  // Create a new branch pointing at the current HEAD
  let branch = repo.branch(branch_name, &commit, false)?;

  // Checkout the newly created branch
  repo.set_head(branch.get().name().unwrap())?;
  repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;

  for entry in WalkDir::new(path)
      .into_iter()
      .filter_map(|e| e.ok())
      .filter(|e| {
          let path = e.path().to_string_lossy();
          e.file_type().is_file() && !exclude_dir.is_match(path.as_ref()) && !exclude_file.is_match(path.as_ref())
      }) {
          let file_path = entry.path();
          if let Err(e) = search_and_replace_in_file(file_path, search_pattern, replace_with) {
              println!("Error: {}", e);
          }
      }

  Ok(())
}

pub fn commit_changes(repo: &Repository, message: &str) -> Result<(), git2::Error> {
  let mut index = repo.index()?;
  let res = index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None);
  match res {
      Ok(res) => println!("Added all files to index {:?}", res),
      Err(e) => println!("Error: {}", e),
  }
  index.write()?;
  let oid = index.write_tree()?;
  let tree = repo.find_tree(oid)?;
  let sig = Signature::now(
      env::var("GITHUB_USERNAME").expect("GITHUB_USERNAME not found in .env").as_str(), 
      env::var("GITHUB_EMAIL").expect("GITHUB_EMAIL not found in .env").as_str()
  )?;
  let parent_commit = repo.head()?.peel_to_commit()?;
  repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent_commit])?;
  Ok(())
}

pub fn push_to_remote(repo: &Repository, pat: &str, branch_name: &str) -> Result<(), git2::Error> {
  let mut remote = repo.find_remote("origin")?;
  let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);
  
  let mut callbacks = RemoteCallbacks::new();
  callbacks.credentials(|_, _, _| Cred::userpass_plaintext(env::var("GITHUB_USERNAME").expect("GITHUB_USERNAME not found in .env").as_str(), pat));

  let mut opts = PushOptions::new();
  opts.remote_callbacks(callbacks);

  remote.push(&[&refspec], Some(&mut opts))?;

  Ok(())
}

pub async fn create_pull_request(
  repo: &str,
  head: &str,
  base: &str,
  title: &str,
  body: &str,
  token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  let client = reqwest::Client::new();
  let pr_url = format!("https://api.github.com/repos/republik-io/{repo}/pulls");

  let response = client.post(&pr_url)
      .bearer_auth(token)
      .header("User-Agent", "Update repos")
      .json(&serde_json::json!({
          "title": title,
          "body": body,
          "head": head,
          "base": base,
      }))
      .send()
      .await?;

  if response.status().is_success() {
      println!("Pull request created successfully.");
  } else {
      eprintln!("Failed to create pull request: {:?}", response.text().await?);
  }

  Ok(())
}

//! What a launch asks of PowerKit.
//!
//! PowerKit runs once per user. A second launch (from the Start menu, a
//! script or the file manager's context menu) hands its request to the
//! running instance and exits; the first launch carries out its own. Either
//! way the command line means the same thing, and [`Request::parse`] is the
//! one place that reads it. How a request reaches the running instance is the
//! platform's business.
use powerkit_core::{always_on, image_resizer, media_renamer};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Nothing specific: bring the settings window forward.
    Open,
    AlwaysOn(always_on::Request),
    /// Open the Image Resizer dialog for these files; none asks for a picker.
    Resize(Vec<PathBuf>),
    /// Replace the MediaRenamer workspace with these files and folders.
    Rename(Vec<PathBuf>),
}
impl Request {
    /// Reads a command line (without the program name).
    ///
    /// `--resize FILE..` and `--rename PATH..` name their paths inline;
    /// `--resize-list FILE` and `--rename-list FILE` name a list the file
    /// manager integration wrote for a long selection. `read_list` reads such
    /// a list, up to the given number of paths, and disposes of it. A resize
    /// request wins over a rename request, and either over `--always-on`.
    pub fn parse(args: &[String], mut read_list: impl FnMut(&Path, usize) -> Vec<PathBuf>) -> Self {
        if let Some(list) = value_of(args, "--resize-list") {
            return Self::Resize(read_list(Path::new(list), image_resizer::MAX_BATCH));
        }
        // Resize paths end at the next option of any kind.
        if let Some(files) = inline_paths(args, "--resize", "-") {
            return Self::Resize(files);
        }
        if args.iter().any(|arg| arg == "--rename-list") {
            if let Some(list) = value_of(args, "--rename-list") {
                return Self::Rename(read_list(Path::new(list), media_renamer::MAX_BATCH));
            }
        } else if let Some(paths) = inline_paths(args, "--rename", "--") {
            return Self::Rename(paths);
        }
        match always_on::Request::parse(args) {
            Some(request) => Self::AlwaysOn(request),
            None => Self::Open,
        }
    }
}
fn value_of<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == option)?;
    args.get(index + 1).map(String::as_str)
}
/// The arguments after `option` up to the first one that starts with `stop`.
fn inline_paths(args: &[String], option: &str, stop: &str) -> Option<Vec<PathBuf>> {
    let index = args.iter().position(|arg| arg == option)?;
    Some(
        args.iter()
            .skip(index + 1)
            .take_while(|arg| !arg.starts_with(stop))
            .filter(|arg| !arg.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }
    fn parse(items: &[&str]) -> Request {
        Request::parse(&args(items), |_, _| panic!("no list was named"))
    }
    fn paths(items: &[&str]) -> Vec<PathBuf> {
        items.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_launch_without_a_request_opens_the_window() {
        assert_eq!(parse(&[]), Request::Open);
        assert_eq!(parse(&["--background"]), Request::Open);
        // A malformed AlwaysOn request asks for nothing.
        assert_eq!(parse(&["--always-on", "soon"]), Request::Open);
    }

    #[test]
    fn resize_arguments_keep_files_and_stop_at_the_next_option() {
        assert_eq!(
            parse(&["--resize", "a.jpg", "b.png"]),
            Request::Resize(paths(&["a.jpg", "b.png"]))
        );
        assert_eq!(
            parse(&["--resize", "a.jpg", "--smoke-test"]),
            Request::Resize(paths(&["a.jpg"]))
        );
        // No file asks for the picker.
        assert_eq!(parse(&["--resize"]), Request::Resize(Vec::new()));
    }

    #[test]
    fn rename_arguments_take_files_and_folders() {
        assert_eq!(
            parse(&["--rename", r"C:\a\b.jpg", r"C:\a\photos"]),
            Request::Rename(paths(&[r"C:\a\b.jpg", r"C:\a\photos"]))
        );
        assert_eq!(parse(&["--rename"]), Request::Rename(Vec::new()));
        // A path may start with a single dash; only `--` ends the list.
        assert_eq!(
            parse(&["--rename", "-draft.jpg", "--background"]),
            Request::Rename(paths(&["-draft.jpg"]))
        );
    }

    #[test]
    fn lists_are_read_with_the_limit_of_their_tool() {
        let mut seen = Vec::new();
        let request = Request::parse(&args(&["--resize-list", "r.txt"]), |path, limit| {
            seen.push((path.to_path_buf(), limit));
            paths(&["a.jpg"])
        });
        assert_eq!(request, Request::Resize(paths(&["a.jpg"])));
        let request = Request::parse(&args(&["--rename-list", "n.txt"]), |path, limit| {
            seen.push((path.to_path_buf(), limit));
            Vec::new()
        });
        assert_eq!(request, Request::Rename(Vec::new()));
        assert_eq!(
            seen,
            [
                (PathBuf::from("r.txt"), image_resizer::MAX_BATCH),
                (PathBuf::from("n.txt"), media_renamer::MAX_BATCH),
            ]
        );
        // A list option without its file names no request.
        assert_eq!(parse(&["--rename-list"]), Request::Open);
    }

    #[test]
    fn always_on_requests_pass_through_and_file_requests_win() {
        assert_eq!(
            parse(&["--always-on", "90"]),
            Request::AlwaysOn(always_on::Request::Minutes(90))
        );
        assert_eq!(
            parse(&["--always-on-pid", "4242"]),
            Request::AlwaysOn(always_on::Request::Process(4242))
        );
        assert_eq!(
            parse(&["--always-on", "90", "--resize", "a.jpg"]),
            Request::Resize(paths(&["a.jpg"]))
        );
    }
}

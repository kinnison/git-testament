use lazy_static::lazy_static;
use rand::{thread_rng, Rng};
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::{Command, Stdio};
use tempfile::Builder;
use tempfile::TempDir;

pub struct TestSentinel {
    dir: Option<TempDir>,
    env: HashMap<String, String>,
    prog_name: String,
}

impl Drop for TestSentinel {
    fn drop(&mut self) {
        self.run_cmd("cargo", &["clean", "-p", &self.prog_name]);
        if env::var("DO_NOT_ERASE_TESTS").is_ok() {
            self.dir.take().unwrap().into_path();
        }
    }
}

pub struct ManifestParts {
    tag: String,
    distance: usize,
    commit: String,
    #[allow(dead_code)]
    date: String,
    dirty: Option<usize>,
}

lazy_static! {
    static ref MANIFEST_RE: Regex = Regex::new(
        r"^([^ ]+) \(([0-9a-f]{9}) (\d{4}-\d\d-\d\d)\)(?: dirty (\d+) modifications?)?$"
    )
    .unwrap();
    static ref TAG_WITH_DISTANCE: Regex = Regex::new(r"^(.+)\+(\d+)$").unwrap();
}

pub fn prep_test(name: &str) -> TestSentinel {
    let outdir = Builder::new()
        .prefix(&format!("test-{}-", name))
        .tempdir_in(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned()))
        .expect("Unable to create temporary directory for test");

    let mut rng = thread_rng();
    let mut name = (0..10)
        .map(|_| rng.sample(rand::distributions::Alphanumeric))
        .map(|c| c as char)
        .collect::<String>();
    name.make_ascii_lowercase();
    let name = format!("gtt-{}", name);

    // Copy the contents of the test template in
    fs::create_dir(outdir.path().join("src")).expect("Unable to make src/ dir");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/test-template/src/main.rs"),
        outdir.path().join("src/main.rs"),
    )
    .expect("Unable to copy main.rs in");
    let toml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test-template/Cargo.toml"
    ));
    let toml = toml.replace("name = \"test2\"", &format!("name = \"{}\"", name));
    fs::write(
        outdir.path().join("Cargo.toml"),
        format!(
            "{}\ngit-testament = {{ path=\"{}\" }}\n",
            toml,
            env::var("CARGO_MANIFEST_DIR")
                .unwrap_or_else(|_| ".".to_owned())
                .replace("\\", "\\\\")
        ),
    )
    .expect("Unable to write Cargo.toml for test");
    println!(
        "Wrote test Cargo.toml:\n{}",
        fs::read_to_string(outdir.path().join("Cargo.toml"))
            .expect("Cannot re-read Cargo.toml for test")
    );
    fs::create_dir(outdir.path().join(".cargo")).expect("Unable to make .cargo/");
    fs::write(
        outdir.path().join(".cargo/config"),
        format!(
            "[build]\ntarget-dir=\"{}/target\"",
            env::var("CARGO_MANIFEST_DIR")
                .unwrap_or_else(|_| "..".to_owned())
                .replace("\\", "\\\\")
        ),
    )
    .expect("Unable to write .cargo/config");
    TestSentinel {
        dir: Some(outdir),
        prog_name: name,
        env: HashMap::new(),
    }
}

impl TestSentinel {
    pub fn setenv(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_owned(), value.to_owned());
    }

    pub fn run_cmd(&self, cmd: &str, args: &[&str]) -> bool {
        let mut child = Command::new(cmd);
        child.args(args).env(
            "GIT_CEILING_DIRECTORIES",
            self.dir.as_ref().unwrap().path().parent().unwrap(),
        );

        for (key, value) in self.env.iter() {
            child.env(key, value);
        }

        let child = child
            .current_dir(self.dir.as_ref().unwrap().path())
            .stdin(Stdio::null())
            .output()
            .expect("Unable to run subcommand");
        if !child.status.success() {
            println!("Failed to run {} {:?}", cmd, args);
            println!("Status was: {:?}", child.status.code());
            println!("Stdout was:\n{:?}", String::from_utf8(child.stdout));
            println!("Stderr was:\n{:?}", String::from_utf8(child.stderr));
        }
        child.status.success()
    }

    pub fn run_cmds(&self, cmds: &[(&str, &[&str])]) -> bool {
        cmds.iter().all(|(cmd, args)| self.run_cmd(cmd, args))
    }

    pub fn basic_git_init(&self) -> bool {
        self.run_cmds(&[
            ("git", &["init"]),
            ("git", &["config", "user.name", "Git Testament Test Suite"]),
            (
                "git",
                &["config", "user.email", "git.testament@digital-scurf.org"],
            ),
            ("git", &["config", "commit.gpgsign", "false"]),
        ])
    }

    pub fn get_output(&self, cmd: &str, args: &[&str]) -> Option<String> {
        let res = Command::new(cmd)
            .env(
                "GIT_CEILING_DIRECTORIES",
                self.dir.as_ref().unwrap().path().parent().unwrap(),
            )
            .current_dir(self.dir.as_ref().unwrap().path())
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("Unable to run subcommand");
        if res.status.success() {
            String::from_utf8(res.stdout).ok()
        } else {
            println!(
                "Attempt to get output of {} {:?} failed: {:?}",
                cmd,
                args,
                res.status.code()
            );
            println!("Output: {:?}", String::from_utf8(res.stdout));
            println!("Error: {:?}", String::from_utf8(res.stderr));
            None
        }
    }

    pub fn get_manifest(&self) -> Option<String> {
        self.get_output(
            &format!(
                "{}/target/debug/{}",
                env::var("CARGO_MANIFEST_DIR").expect("Unable to run without CARGO_MANIFEST_DIR"),
                self.prog_name
            ),
            &[],
        )
    }

    pub fn get_manifest_parts(&self) -> ManifestParts {
        let output = self
            .get_manifest()
            .expect("Unable to retrieve full manifest support");
        let first = output
            .lines()
            .next()
            .expect("Unable to retrieve manifest line");
        let caps = MANIFEST_RE
            .captures(first)
            .unwrap_or_else(|| panic!("Unable to parse manifest line: '{}'", first));
        // Step one, process the tag bit
        let (tag, distance) = if let Some(tcaps) =
            TAG_WITH_DISTANCE.captures(caps.get(1).expect("No tag captures?").as_str())
        {
            (
                tcaps.get(1).expect("No tag capture?").as_str().to_owned(),
                tcaps
                    .get(2)
                    .expect("No distance capture?")
                    .as_str()
                    .parse::<usize>()
                    .expect("Unable to parse distance"),
            )
        } else {
            (caps.get(1).unwrap().as_str().to_owned(), 0usize)
        };

        let dirty = caps.get(4).map(|dirtycap| {
            dirtycap
                .as_str()
                .parse::<usize>()
                .expect("Unable to parse dirty count")
        });

        ManifestParts {
            tag,
            distance,
            commit: caps
                .get(2)
                .expect("Unable to extract commit")
                .as_str()
                .to_owned(),
            date: caps
                .get(3)
                .expect("Unable to extract date")
                .as_str()
                .to_owned(),
            dirty,
        }
    }

    #[allow(dead_code)]
    pub fn assert_manifest_exact(&self, manifest: &str) {
        let output = self
            .get_manifest()
            .expect("Unable to retrieve full manifest output");
        let first = output
            .lines()
            .next()
            .expect("Unable to retrieve manifest line");
        assert_eq!(first, manifest);
    }

    pub fn assert_manifest_parts(
        &self,
        tagname: &str,
        distance: usize,
        _date: &str,
        dirty: Option<usize>,
    ) {
        let manifest = self.get_manifest_parts();
        let curcommit = self
            .get_output("git", &["rev-parse", "HEAD"])
            .expect("Unable to get HEAD commit");
        assert_eq!(manifest.tag, tagname);
        assert_eq!(manifest.distance, distance);
        assert_eq!(&curcommit[..manifest.commit.len()], manifest.commit);
        // TODO: Find some sensible way to assert the date

        assert_eq!(dirty, manifest.dirty);
    }

    pub fn assert_manifest_contains(&self, substr: &str) {
        let manifest = self.get_manifest().expect("Unable to retrieve manifest");
        println!("Retrieved manifest: {:?}", manifest);
        println!("Does it contain: {:?}", substr);
        assert!(manifest.find(substr) != None);
    }

    pub fn dirty_code(&self) {
        let main_rs = self.dir.as_ref().unwrap().path().join("src/main.rs");
        let code = fs::read_to_string(&main_rs).expect("Unable to read code");
        fs::write(main_rs, format!("{}\n\n", code)).expect("Unable to write code");
    }
}

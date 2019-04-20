use lazy_static::lazy_static;
use rand::{thread_rng, Rng};
use regex::Regex;
use std::env;
use std::fs;
use std::process::{Command, Stdio};
use tempdir::TempDir;

pub struct TestSentinel {
    dir: Option<TempDir>,
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
    let outdir = TempDir::new_in(
        env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned()),
        &format!("test-{}-", name),
    )
    .expect("Unable to create temporary directory for test");

    let mut rng = thread_rng();
    let mut name = (0..10)
        .map(|_| rng.sample(rand::distributions::Alphanumeric))
        .collect::<String>();
    name.make_ascii_lowercase();

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
            env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned())
        ),
    )
    .expect("Unable to write Cargo.toml for test");
    fs::create_dir(outdir.path().join(".cargo")).expect("Unable to make .cargo/");
    fs::write(
        outdir.path().join(".cargo/config"),
        format!(
            "[build]\ntarget-dir=\"{}/target\"",
            env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| "..".to_owned())
        ),
    )
    .expect("Unable to write .cargo/config");
    TestSentinel {
        dir: Some(outdir),
        prog_name: name,
    }
}

impl TestSentinel {
    pub fn run_cmd(&self, cmd: &str, args: &[&str]) -> bool {
        let mut child = Command::new(cmd)
            .args(args)
            .env(
                "GIT_CEILING_DIRECTORIES",
                self.dir.as_ref().unwrap().path().parent().unwrap(),
            )
            .current_dir(self.dir.as_ref().unwrap().path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Unable to run subcommand");
        let ecode = child.wait().expect("Unable to wait for child");
        ecode.success()
    }

    pub fn run_cmds(&self, cmds: &[(&str, &[&str])]) -> bool {
        for (cmd, args) in cmds.iter() {
            if self.run_cmd(cmd, args) == false {
                return false;
            }
        }
        true
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
            .expect(&format!("Unable to parse manifest line: '{}'", first));
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

        let dirty = if let Some(dirtycap) = caps.get(4) {
            Some(
                dirtycap
                    .as_str()
                    .parse::<usize>()
                    .expect("Unable to parse dirty count"),
            )
        } else {
            None
        };

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
        assert!(manifest.find(substr) != None);
    }

    pub fn dirty_code(&self) {
        let main_rs = self.dir.as_ref().unwrap().path().join("src/main.rs");
        let code = fs::read_to_string(&main_rs).expect("Unable to read code");
        fs::write(main_rs, format!("{}\n\n", code)).expect("Unable to write code");
    }
}

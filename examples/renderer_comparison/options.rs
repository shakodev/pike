use std::{io, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Options {
    pub output: PathBuf,
    pub visible: usize,
    pub hidden: usize,
    pub saturated: bool,
    pub flood: bool,
    pub seconds: u64,
    pub settle: u64,
    pub probes: usize,
    pub synthetic_focus: bool,
}

impl Options {
    pub fn parse(args: impl Iterator<Item = String>) -> io::Result<Self> {
        let mut value = Self {
            output: PathBuf::new(),
            visible: 1,
            hidden: 0,
            saturated: false,
            flood: false,
            seconds: 60,
            settle: 10,
            probes: 0,
            synthetic_focus: false,
        };
        let mut args = args;
        while let Some(key) = args.next() {
            let argument = args
                .next()
                .ok_or_else(|| io::Error::other("expected key/value pairs"))?;
            let number = || argument.parse::<u64>().map_err(io::Error::other);
            match key.as_str() {
                "--output" => value.output = argument.into(),
                "--visible" => value.visible = number()? as usize,
                "--hidden" => value.hidden = number()? as usize,
                "--history" if argument == "empty" || argument == "saturated" => {
                    value.saturated = argument == "saturated"
                }
                "--workload" if argument == "idle" || argument == "flood" => {
                    value.flood = argument == "flood"
                }
                "--seconds" => value.seconds = number()?,
                "--settle" => value.settle = number()?,
                "--synthetic-focus" if argument == "true" || argument == "false" => {
                    value.synthetic_focus = argument == "true"
                }
                "--probes" => value.probes = number()? as usize,
                _ => return Err(io::Error::other("unsupported renderer comparison option")),
            }
        }
        if value.output.as_os_str().is_empty()
            || ![1, 4, 8].contains(&value.visible)
            || ![0, 8].contains(&value.hidden)
            || !(1..=600).contains(&value.seconds)
            || (cfg!(windows) && value.synthetic_focus)
            || value.settle > 30
            || value.probes > 200
            || (!value.flood && value.probes != 0)
        {
            return Err(io::Error::other(
                "comparison bounds: visible=1/4/8 hidden=0/8 seconds=1..600 settle=0..30 probes=0..200 (flood only)",
            ));
        }
        Ok(value)
    }

    /// Breadth-first split order preserves usable 4×2 panes for eight sessions.
    pub fn splits(&self) -> Vec<(usize, pike::app::Direction)> {
        use pike::app::Direction::{Down, Right};
        [
            (0, Right),
            (0, Down),
            (1, Down),
            (0, Right),
            (1, Right),
            (2, Right),
            (3, Right),
        ]
        .into_iter()
        .take(self.visible - 1)
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &str) -> io::Result<Options> {
        Options::parse(s.split_whitespace().map(str::to_owned))
    }
    #[test]
    fn rejects_unbounded_or_ambiguous_scenarios() {
        for s in [
            "--visible 32",
            "--hidden 32",
            "--seconds 0",
            "--seconds 601",
            "--probes 201",
            "--probes 1",
            "--settle 31",
            "--history secret",
            "--output",
        ] {
            assert!(parse(&format!("--output fresh {s}")).is_err(), "{s}");
        }
        assert!(parse("").is_err());
    }
    #[test]
    fn scenario_and_split_bounds_match_the_native_matrix() {
        let value = parse("--output fresh --visible 8 --hidden 8 --history saturated --workload flood --seconds 600 --probes 200").unwrap();
        assert_eq!(value.splits().len(), 7);
        assert!(
            value
                .splits()
                .iter()
                .enumerate()
                .all(|(i, (source, _))| *source <= i)
        );
    }
}

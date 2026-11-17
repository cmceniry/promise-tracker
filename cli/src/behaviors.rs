use clap::Parser;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) or dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show SuperAgent members as agents of their own, so promises between
    /// members are not reduced away
    #[clap(long)]
    unfold: bool,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    let mut behaviors: Vec<String> = tracker.get_working_behaviors().into_iter().collect();
    behaviors.sort();
    for behavior_name in behaviors {
        println!("{}", behavior_name);
    }
    // Patterns are not behaviors — each stands for a family of them — so they
    // are listed apart and marked.
    let mut patterns: Vec<String> = tracker.get_behavior_patterns().into_iter().collect();
    patterns.sort();
    for pattern in patterns {
        println!("{}\t(pattern)", pattern);
    }
}

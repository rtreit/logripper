use logripper_core::proto::logripper;

fn main() {
    println!("LogRipper TUI — coming soon");
    // Verify proto types are accessible
    let _band = logripper::domain::Band::Band20m;
    println!("Proto types loaded successfully");
}

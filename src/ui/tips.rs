const TIPS: &str = include_str!("../../assets/tips.txt");

pub(super) fn pick(previous: Option<&str>) -> &'static str {
    let candidates = || TIPS.lines().filter(|tip| Some(*tip) != previous);
    let count = candidates().count();
    let mut bytes = [0; 8];

    let random = if getrandom::fill(&mut bytes).is_ok() {
        u64::from_ne_bytes(bytes)
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    };

    candidates()
        .nth((random % count as u64) as usize)
        .unwrap_or("♪")
}

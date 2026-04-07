use crate::models::ProcessFamily;

pub fn detect_family(name: &str, command: &str) -> ProcessFamily {
    let n = name.to_ascii_lowercase();
    let c = command.to_ascii_lowercase();
    let hay = format!("{n} {c}");

    if hay.contains("playwright")
        || hay.contains("headless")
        || hay.contains("openchrome")
        || hay.contains("chrome-mcp")
    {
        return ProcessFamily::BrowserAutomation;
    }

    if hay.contains("google chrome") || hay.contains("chromium") || hay.contains("firefox") {
        return ProcessFamily::BrowserMain;
    }

    if hay.contains("codex") || hay.contains("claude") {
        return ProcessFamily::Agent;
    }

    if hay.contains("tmux") {
        return ProcessFamily::Multiplexer;
    }

    if hay.contains("webpack")
        || hay.contains("next build")
        || hay.contains("cargo")
        || hay.contains("rustc")
        || hay.contains("pytest")
        || hay.contains("jest")
    {
        return ProcessFamily::BuildTool;
    }

    if hay.contains("watch")
        || hay.contains("nodemon")
        || hay.contains("tail -f")
        || hay.contains("fswatch")
    {
        return ProcessFamily::Watcher;
    }

    if hay.contains("mcp")
        || hay.contains("helper")
        || hay.contains("server")
        || hay.contains("bridge")
    {
        return ProcessFamily::Helper;
    }

    ProcessFamily::Unknown
}

#[cfg(test)]
mod tests {
    use super::detect_family;
    use crate::models::ProcessFamily;

    #[test]
    fn fingerprints_playwright() {
        assert_eq!(
            detect_family("node", "node ./node_modules/.bin/playwright test"),
            ProcessFamily::BrowserAutomation
        );
    }

    #[test]
    fn fingerprints_tmux() {
        assert_eq!(
            detect_family("tmux: server", "tmux"),
            ProcessFamily::Multiplexer
        );
    }
}

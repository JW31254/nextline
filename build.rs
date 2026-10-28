fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "NextLine");
        res.set("FileDescription", "NextLine - scripted typing for live demos");
        if let Err(error) = res.compile() {
            // The app still works without an embedded icon (e.g. when the Windows SDK resource compiler is missing).
            println!("cargo:warning=could not embed the exe icon: {error}");
        }
    }
}

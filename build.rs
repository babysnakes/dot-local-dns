fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("resources/Icon.ico"); // Path to your .ico file
        res.compile().unwrap();
    }
}
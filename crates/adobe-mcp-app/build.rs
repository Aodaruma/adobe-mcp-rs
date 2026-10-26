fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/adobe-mcp.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("../../assets/icons/adobe-mcp.ico")
        .set("ProductName", "Adobe MCP")
        .set("FileDescription", "Adobe MCP desktop app")
        .compile()
        .expect("compile the Adobe MCP Windows icon resource");
}

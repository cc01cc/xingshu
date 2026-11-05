/// v1 file-type extensions per PLAN-0587 CLI SPEC §7.
const TYPES: &[(&str, &[&str])] = &[
    ("rust", &["rs"]),
    ("go", &["go"]),
    ("typescript", &["ts", "tsx", "mts", "cts"]),
    ("javascript", &["js", "jsx", "mjs", "cjs"]),
    ("python", &["py", "pyi"]),
    ("java", &["java"]),
    ("markdown", &["md", "markdown"]),
    ("json", &["json", "jsonc"]),
    ("yaml", &["yaml", "yml"]),
    ("toml", &["toml"]),
    ("shell", &["sh", "bash", "zsh", "fish", "ps1", "bat", "cmd"]),
    ("vue", &["vue"]),
    ("css", &["css", "scss", "sass", "less"]),
    ("html", &["html", "htm"]),
    ("sql", &["sql"]),
    ("xml", &["xml", "xsd", "xsl", "svg"]),
    ("c", &["c", "h"]),
    ("cpp", &["cc", "cpp", "cxx", "hh", "hpp", "hxx"]),
    ("csharp", &["cs", "csx"]),
    ("ruby", &["rb"]),
    ("php", &["php"]),
    ("swift", &["swift"]),
    ("kotlin", &["kt", "kts"]),
];

/// Known binary extensions excluded from text content (SPEC §7).
const BINARY_EXTENSIONS: &[&str] = &[
    "7z", "a", "avi", "bin", "bmp", "class", "dll", "dylib", "eot", "exe", "flac", "gif", "gz",
    "ico", "jar", "jpeg", "jpg", "m4a", "m4v", "mov", "mp3", "mp4", "o", "otf", "pdf", "png",
    "psd", "so", "sqlite", "sqlite3", "tar", "tif", "tiff", "ttf", "wasm", "wav", "webm", "webp",
    "woff", "woff2", "xls", "xlsx", "zip", "zst",
];

/// Known generated directory components excluded from text content (SPEC §7).
const GENERATED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".vite",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
    "coverage",
];

pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

pub fn extensions_for(type_name: &str) -> Option<&'static [&'static str]> {
    TYPES.iter().find(|(n, _)| *n == type_name).map(|(_, e)| *e)
}

pub fn is_binary_extension(ext: &str) -> bool {
    BINARY_EXTENSIONS.contains(&ext)
}

pub fn is_generated_component(component: &str) -> bool {
    let lower = component.to_ascii_lowercase();
    GENERATED_DIRS.contains(&lower.as_str())
}

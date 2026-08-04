use ratatui::style::Color;

pub struct Language {
    pub id: &'static str,
    pub name: &'static str,
    pub color: Color,
    pub ext: &'static str,
    pub hello: &'static str,
    pub piston: Option<(&'static str, &'static str)>,
    pub keywords: &'static [&'static str],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub static LANGS: &[Language] = &[
    Language {
        id: "python",
        name: "Python",
        color: rgb(0x35, 0x72, 0xa5),
        ext: ".py",
        hello: "print(\"Hello, World!\")\n\nname = input(\"Enter name: \")\nprint(f\"Hello, {name}!\")",
        piston: Some(("python", "3.10.0")),
        keywords: &["print", "input", "len", "range", "int", "str", "float", "list", "dict", "tuple", "set", "if", "else", "elif", "for", "while", "def", "class", "return", "import", "from", "as", "with", "try", "except", "finally", "lambda", "yield", "pass", "break", "continue", "True", "False", "None", "and", "or", "not", "in", "is", "open", "append", "extend", "pop", "remove", "sort", "sorted", "map", "filter", "zip", "enumerate", "type", "isinstance", "hasattr", "getattr", "setattr"],
    },
    Language {
        id: "javascript",
        name: "JS",
        color: rgb(0xf7, 0xdf, 0x1e),
        ext: ".js",
        hello: "console.log(\"Hello, World!\");\n\nconst greet = name => `Hello, ${name}!`;\nconsole.log(greet(\"Nexus\"));",
        piston: Some(("javascript", "18.15.0")),
        keywords: &["console.log", "console.error", "function", "const", "let", "var", "return", "if", "else", "for", "while", "class", "import", "export", "default", "async", "await", "Promise", "then", "catch", "finally", "new", "this", "typeof", "instanceof", "Array", "Object", "String", "Number", "Boolean", "JSON", "Math", "Date", "setTimeout", "setInterval", "document", "window", "fetch", "map", "filter", "reduce", "forEach", "find", "push", "pop", "shift", "unshift", "splice", "slice", "join", "split", "includes", "indexOf"],
    },
    Language {
        id: "typescript",
        name: "TypeScript",
        color: rgb(0x31, 0x78, 0xc6),
        ext: ".ts",
        hello: "const greet = (name: string): string => `Hello, ${name}!`;\nconsole.log(greet(\"World\"));",
        piston: Some(("typescript", "5.0.3")),
        keywords: &["function", "const", "let", "var", "return", "if", "else", "for", "while", "class", "import", "export", "default", "async", "await", "new", "this", "typeof", "instanceof", "true", "false", "null", "undefined", "switch", "case", "break", "continue", "throw", "try", "catch", "finally", "interface", "type", "enum", "namespace", "declare", "implements", "extends", "abstract", "public", "private", "protected", "readonly", "string", "number", "boolean", "any", "void", "never"],
    },
    Language {
        id: "cpp",
        name: "C++",
        color: rgb(0xf3, 0x4b, 0x7d),
        ext: ".cpp",
        hello: "#include <iostream>\nusing namespace std;\n\nint main() {\n  cout << \"Hello, World!\" << endl;\n  return 0;\n}",
        piston: Some(("c++", "10.2.0")),
        keywords: &["cout", "cin", "endl", "#include", "using", "namespace", "std", "int", "float", "double", "char", "bool", "void", "string", "vector", "map", "pair", "auto", "return", "if", "else", "for", "while", "class", "struct", "public", "private", "protected", "virtual", "override", "nullptr", "true", "false", "template", "typename"],
    },
    Language {
        id: "c",
        name: "C",
        color: rgb(0x55, 0x55, 0x55),
        ext: ".c",
        hello: "#include <stdio.h>\n\nint main() {\n  printf(\"Hello, World!\\n\");\n  return 0;\n}",
        piston: Some(("c", "10.2.0")),
        keywords: &["int", "float", "double", "char", "void", "const", "static", "if", "else", "for", "while", "do", "return", "struct", "typedef", "enum", "switch", "case", "break", "continue", "sizeof", "NULL", "include", "define"],
    },
    Language {
        id: "java",
        name: "Java",
        color: rgb(0xb0, 0x72, 0x19),
        ext: ".java",
        hello: "public class Main {\n  public static void main(String[] args) {\n    System.out.println(\"Hello, World!\");\n  }\n}",
        piston: Some(("java", "15.0.2")),
        keywords: &["System.out.println", "System.out.print", "public", "private", "protected", "static", "void", "int", "double", "float", "char", "boolean", "String", "class", "interface", "extends", "implements", "new", "return", "if", "else", "for", "while", "switch", "case", "break", "continue", "import", "package", "try", "catch", "finally", "throw", "throws", "ArrayList", "HashMap", "List", "Map"],
    },
    Language {
        id: "go",
        name: "Go",
        color: rgb(0x00, 0xad, 0xd8),
        ext: ".go",
        hello: "package main\n\nimport \"fmt\"\n\nfunc main() {\n  fmt.Println(\"Hello, World!\")\n}",
        piston: Some(("go", "1.16.2")),
        keywords: &["fmt.Println", "fmt.Printf", "fmt.Sprintf", "func", "var", "const", "type", "struct", "interface", "map", "slice", "chan", "go", "defer", "select", "range", "return", "if", "else", "for", "switch", "case", "break", "continue", "import", "package", "make", "append", "len", "cap", "close", "copy", "delete", "new", "panic", "recover", "error", "nil", "true", "false"],
    },
    Language {
        id: "bash",
        name: "Bash",
        color: rgb(0x89, 0xe0, 0x51),
        ext: ".sh",
        hello: "#!/bin/bash\necho \"Hello, World!\"\n\nread -p \"Enter name: \" name\necho \"Hello, $name!\"",
        piston: Some(("bash", "5.2.0")),
        keywords: &["echo", "read", "if", "then", "else", "fi", "for", "while", "do", "done", "case", "esac", "function", "return", "exit", "export", "local", "declare", "printf", "grep", "sed", "awk", "cat", "ls", "cd", "pwd", "mkdir", "rm", "cp", "mv", "chmod", "chown", "source", "$?", "$0", "$1", "$#", "$@"],
    },
    Language {
        id: "rust",
        name: "Rust",
        color: rgb(0xde, 0xa5, 0x84),
        ext: ".rs",
        hello: "fn main() {\n  println!(\"Hello, World!\");\n  let greeting = |name: &str| format!(\"Hello, {}!\", name);\n  println!(\"{}\", greeting(\"Nexus\"));\n}",
        piston: Some(("rust", "1.50.0")),
        keywords: &["println!", "print!", "fn", "let", "mut", "const", "struct", "enum", "impl", "trait", "use", "mod", "pub", "return", "if", "else", "for", "while", "loop", "match", "break", "continue", "Vec", "String", "HashMap", "Option", "Result", "Some", "None", "Ok", "Err", "Box", "Arc", "Rc", "move", "clone", "unwrap", "expect"],
    },
    Language {
        id: "ruby",
        name: "Ruby",
        color: rgb(0x70, 0x15, 0x16),
        ext: ".rb",
        hello: "puts \"Hello, World!\"\n\ndef greet(name)\n  \"Hello, #{name}!\"\nend\n\nputs greet(\"Nexus\")",
        piston: Some(("ruby", "3.0.1")),
        keywords: &["def", "class", "module", "if", "elsif", "else", "unless", "while", "until", "for", "do", "end", "return", "require", "include", "extend", "new", "self", "true", "false", "nil", "and", "or", "not", "in", "begin", "rescue", "ensure", "raise", "yield", "lambda", "proc"],
    },
    Language {
        id: "php",
        name: "PHP",
        color: rgb(0x4f, 0x5d, 0x95),
        ext: ".php",
        hello: "<?php\necho \"Hello, World!\\n\";\n\n$greet = fn($name) => \"Hello, $name!\";\necho $greet(\"Nexus\");",
        piston: Some(("php", "8.2.3")),
        keywords: &["function", "class", "if", "else", "elseif", "while", "for", "foreach", "return", "echo", "print", "new", "this", "public", "private", "protected", "static", "namespace", "use", "require", "include", "try", "catch", "finally", "throw", "true", "false", "null", "array", "string", "int", "float", "bool"],
    },
    Language {
        id: "swift",
        name: "Swift",
        color: rgb(0xf0, 0x51, 0x38),
        ext: ".swift",
        hello: "import Foundation\n\nprint(\"Hello, World!\")\n\nfunc greet(_ name: String) -> String {\n  return \"Hello, \\(name)!\"\n}\nprint(greet(\"Nexus\"))",
        piston: Some(("swift", "5.3.3")),
        keywords: &["func", "var", "let", "class", "struct", "enum", "protocol", "extension", "if", "else", "guard", "for", "while", "switch", "case", "return", "import", "init", "self", "super", "true", "false", "nil", "throws", "throw", "try", "catch", "async", "await", "override", "final", "public", "private", "internal", "weak", "lazy", "in", "where"],
    },
];

pub fn by_id(id: &str) -> Option<&'static Language> {
    LANGS.iter().find(|l| l.id == id)
}

pub fn by_ext(ext: &str) -> Option<&'static Language> {
    LANGS.iter().find(|l| l.ext == ext)
}

pub fn default_lang() -> &'static Language {
    &LANGS[0]
}

pub fn guess_lang(name: &str) -> &'static Language {
    let ext = name.rsplit('.').next().unwrap_or("");
    by_ext(&format!(".{ext}")).or_else(|| by_ext(ext)).unwrap_or_else(default_lang)
}

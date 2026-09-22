//! The catalogue of categories and tools, in the design's order.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cat {
    Conv,
    Enc,
    Fmt,
    Gen,
    Gfx,
    Test,
    Text,
}

pub struct CatInfo {
    pub id: Cat,
    pub key: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

pub const CATS: &[CatInfo] = &[
    CatInfo { id: Cat::Conv, key: "conv", label: "Converters", icon: "conv" },
    CatInfo { id: Cat::Enc, key: "enc", label: "Encoders / Decoders", icon: "enc" },
    CatInfo { id: Cat::Fmt, key: "fmt", label: "Formatters", icon: "fmt" },
    CatInfo { id: Cat::Gen, key: "gen", label: "Generators", icon: "gen" },
    CatInfo { id: Cat::Gfx, key: "gfx", label: "Graphic", icon: "gfx" },
    CatInfo { id: Cat::Test, key: "test", label: "Testers", icon: "test" },
    CatInfo { id: Cat::Text, key: "text", label: "Text", icon: "text" },
];

pub fn cat(id: Cat) -> &'static CatInfo {
    CATS.iter().find(|c| c.id == id).unwrap()
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToolId {
    JsonYaml,
    NumBase,
    Date,
    Base64,
    Url,
    Html,
    Jwt,
    JsonFmt,
    Hash,
    Uuid,
    Password,
    Lorem,
    Color,
    Regex,
    TextCase,
    Escape,
}

pub struct Tool {
    pub id: ToolId,
    pub key: &'static str,
    pub cat: Cat,
    pub name: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub icon: &'static str,
    /// Extra words the command palette matches on.
    pub keywords: &'static str,
}

pub const TOOLS: &[Tool] = &[
    Tool { id: ToolId::JsonYaml, key: "jsonyaml", cat: Cat::Conv, name: "JSON to YAML", title: "JSON to YAML Converter", desc: "Convert JSON data into clean, human-readable YAML.", icon: "swap", keywords: "yml config" },
    Tool { id: ToolId::NumBase, key: "numbase", cat: Cat::Conv, name: "Number Base", title: "Number Base Converter", desc: "Convert numbers between hexadecimal, decimal, octal and binary.", icon: "hash", keywords: "hex binary octal radix bits" },
    Tool { id: ToolId::Date, key: "date", cat: Cat::Conv, name: "Date", title: "Date Converter", desc: "Convert between Unix timestamps and readable dates.", icon: "cal", keywords: "time epoch unix timestamp iso" },
    Tool { id: ToolId::Base64, key: "base64", cat: Cat::Enc, name: "Base64 Text", title: "Base64 Text Encoder / Decoder", desc: "Encode and decode UTF-8 text to and from Base64.", icon: "b64", keywords: "b64 encode decode" },
    Tool { id: ToolId::Url, key: "url", cat: Cat::Enc, name: "URL", title: "URL Encoder / Decoder", desc: "Percent-encode or decode URL components.", icon: "link", keywords: "percent uri query" },
    Tool { id: ToolId::Html, key: "html", cat: Cat::Enc, name: "HTML", title: "HTML Encoder / Decoder", desc: "Escape or unescape HTML entities.", icon: "html", keywords: "entities escape" },
    Tool { id: ToolId::Jwt, key: "jwt", cat: Cat::Enc, name: "JWT Decoder", title: "JWT Decoder", desc: "Decode a JSON Web Token and inspect its header and claims.", icon: "shield", keywords: "token bearer auth claims" },
    Tool { id: ToolId::JsonFmt, key: "jsonfmt", cat: Cat::Fmt, name: "JSON", title: "JSON Formatter", desc: "Indent, minify, sort and validate JSON.", icon: "braces", keywords: "prettify minify beautify validate" },
    Tool { id: ToolId::Hash, key: "hash", cat: Cat::Gen, name: "Hash / Checksum", title: "Hash Generator", desc: "Compute MD5 and SHA checksums of any text.", icon: "finger", keywords: "md5 sha1 sha256 sha512 checksum digest" },
    Tool { id: ToolId::Uuid, key: "uuid", cat: Cat::Gen, name: "UUID", title: "UUID Generator", desc: "Generate random version 4 UUIDs in bulk.", icon: "uuid", keywords: "guid id random" },
    Tool { id: ToolId::Password, key: "password", cat: Cat::Gen, name: "Password", title: "Password Generator", desc: "Create strong random passwords.", icon: "lock", keywords: "secret random" },
    Tool { id: ToolId::Lorem, key: "lorem", cat: Cat::Gen, name: "Lorem Ipsum", title: "Lorem Ipsum Generator", desc: "Generate placeholder text for layouts.", icon: "lines", keywords: "placeholder dummy text" },
    Tool { id: ToolId::Color, key: "color", cat: Cat::Gfx, name: "Color Converter", title: "Color Converter", desc: "Convert HEX, RGB, HSL and HSV and check contrast.", icon: "drop", keywords: "colour hex rgb hsl hsv contrast" },
    Tool { id: ToolId::Regex, key: "regex", cat: Cat::Test, name: "Regular Expression", title: "Regular Expression Tester", desc: "Test patterns against sample text in real time.", icon: "regex", keywords: "regexp pattern match" },
    Tool { id: ToolId::TextCase, key: "textcase", cat: Cat::Text, name: "Text Analyzer", title: "Text Analyzer & Case Converter", desc: "Count characters and words, and change text case.", icon: "case", keywords: "camel snake kebab upper lower count words" },
    Tool { id: ToolId::Escape, key: "escape", cat: Cat::Text, name: "Escape / Unescape", title: "Text Escape / Unescape", desc: "Escape or unescape strings for JSON and code.", icon: "slash", keywords: "backslash string json" },
];

pub fn tool(id: ToolId) -> &'static Tool {
    TOOLS.iter().find(|t| t.id == id).unwrap()
}

impl Tool {
    /// The design's search: substring over name, title and description.
    pub fn matches(&self, q: &str) -> bool {
        if q.is_empty() {
            return true;
        }
        let hay = format!("{} {} {}", self.name, self.title, self.desc).to_lowercase();
        hay.contains(q)
    }
}

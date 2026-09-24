//! The catalogue of categories and tools, in the design's order.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cat {
    Conv,
    Enc,
    Fmt,
    Gen,
    Gfx,
    Net,
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
    CatInfo { id: Cat::Net, key: "net", label: "Network", icon: "net" },
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
    JsonToml,
    Hex,
    UrlParse,
    Lines,
    Unicode,
    Subnet,
    Mac,
    Ula,
    JsonCsv,
    Sql,
    Xml,
    Cron,
    JsonPath,
    TextDiff,
    DataDiff,
    Markdown,
    B64Image,
    Cert,
    Qr,
    Mock,
    IpRange,
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
    Tool { id: ToolId::JsonYaml, key: "jsonyaml", cat: Cat::Conv, name: "JSON ↔ YAML", title: "JSON ↔ YAML Converter", desc: "Convert JSON to clean, human-readable YAML and back.", icon: "swap", keywords: "yml config yaml to json" },
    Tool { id: ToolId::JsonCsv, key: "jsoncsv", cat: Cat::Conv, name: "JSON ↔ CSV", title: "JSON ↔ CSV Converter", desc: "Turn arrays of objects into CSV and back.", icon: "table", keywords: "csv tsv spreadsheet excel sheet" },
    Tool { id: ToolId::JsonToml, key: "jsontoml", cat: Cat::Conv, name: "JSON ↔ TOML", title: "JSON ↔ TOML Converter", desc: "Convert between JSON and TOML configuration files.", icon: "swap", keywords: "toml config cargo pyproject" },
    Tool { id: ToolId::NumBase, key: "numbase", cat: Cat::Conv, name: "Number Base", title: "Number Base Converter", desc: "Convert numbers between hexadecimal, decimal, octal and binary.", icon: "hash", keywords: "hex binary octal radix bits" },
    Tool { id: ToolId::Date, key: "date", cat: Cat::Conv, name: "Date", title: "Date Converter", desc: "Convert between Unix timestamps and readable dates.", icon: "cal", keywords: "time epoch unix timestamp iso" },
    Tool { id: ToolId::Base64, key: "base64", cat: Cat::Enc, name: "Base64 Text", title: "Base64 Text Encoder / Decoder", desc: "Encode and decode UTF-8 text to and from Base64.", icon: "b64", keywords: "b64 encode decode" },
    Tool { id: ToolId::Hex, key: "hex", cat: Cat::Enc, name: "Hex Text", title: "Hex ↔ Text Converter", desc: "Turn text into hexadecimal bytes and back.", icon: "hexa", keywords: "hexadecimal bytes ascii utf8 encode decode" },
    Tool { id: ToolId::B64Image, key: "b64image", cat: Cat::Enc, name: "Base64 Image", title: "Base64 Image Encoder / Decoder", desc: "Turn images into data URIs, and Base64 back into images.", icon: "image", keywords: "data uri png jpg jpeg gif webp svg picture encode decode" },
    Tool { id: ToolId::Url, key: "url", cat: Cat::Enc, name: "URL", title: "URL Encoder / Decoder", desc: "Percent-encode or decode URL components.", icon: "link", keywords: "percent uri query" },
    Tool { id: ToolId::Html, key: "html", cat: Cat::Enc, name: "HTML", title: "HTML Encoder / Decoder", desc: "Escape or unescape HTML entities.", icon: "html", keywords: "entities escape" },
    Tool { id: ToolId::UrlParse, key: "urlparse", cat: Cat::Enc, name: "URL Parser", title: "URL Parser", desc: "Break a URL into its parts and decode its query parameters.", icon: "split", keywords: "uri query string params host port path inspect" },
    Tool { id: ToolId::Cert, key: "cert", cat: Cat::Enc, name: "Certificate", title: "Certificate Decoder", desc: "Inspect X.509 certificates: subject, validity, names and fingerprints.", icon: "cert", keywords: "x509 pem der ssl tls https certificate chain" },
    Tool { id: ToolId::Jwt, key: "jwt", cat: Cat::Enc, name: "JWT Decoder", title: "JWT Decoder", desc: "Decode a JSON Web Token and inspect its header and claims.", icon: "shield", keywords: "token bearer auth claims" },
    Tool { id: ToolId::JsonFmt, key: "jsonfmt", cat: Cat::Fmt, name: "JSON", title: "JSON Formatter", desc: "Indent, minify, sort and validate JSON.", icon: "braces", keywords: "prettify minify beautify validate" },
    Tool { id: ToolId::Sql, key: "sql", cat: Cat::Fmt, name: "SQL", title: "SQL Formatter", desc: "Indent SQL queries, or squash them onto one line.", icon: "db", keywords: "query postgres mysql sqlite beautify prettify" },
    Tool { id: ToolId::Xml, key: "xml", cat: Cat::Fmt, name: "XML", title: "XML Formatter", desc: "Pretty-print or minify XML, and check it is well-formed.", icon: "html", keywords: "validate beautify minify prettify svg plist soap rss" },
    Tool { id: ToolId::Hash, key: "hash", cat: Cat::Gen, name: "Hash / Checksum", title: "Hash Generator", desc: "Compute MD5, SHA and CRC-32 checksums, or HMAC signatures, of any text.", icon: "finger", keywords: "md5 sha1 sha224 sha256 sha384 sha512 crc32 hmac checksum digest signature" },
    Tool { id: ToolId::Uuid, key: "uuid", cat: Cat::Gen, name: "UUID", title: "UUID Generator", desc: "Generate UUID v4, UUID v7 and ULID identifiers in bulk.", icon: "uuid", keywords: "guid id random ulid v7 sortable" },
    Tool { id: ToolId::Password, key: "password", cat: Cat::Gen, name: "Password", title: "Password Generator", desc: "Create strong random passwords.", icon: "lock", keywords: "secret random" },
    Tool { id: ToolId::Qr, key: "qr", cat: Cat::Gen, name: "QR Code", title: "QR Code Generator", desc: "Make QR codes from text or links.", icon: "qr", keywords: "qrcode barcode scan link" },
    Tool { id: ToolId::Mock, key: "mock", cat: Cat::Gen, name: "Mock Data", title: "Mock Data Generator", desc: "Generate fake people, companies and records as JSON or CSV.", icon: "dice", keywords: "fake faker fixture seed sample test data random" },
    Tool { id: ToolId::Lorem, key: "lorem", cat: Cat::Gen, name: "Lorem Ipsum", title: "Lorem Ipsum Generator", desc: "Generate placeholder text for layouts.", icon: "lines", keywords: "placeholder dummy text" },
    Tool { id: ToolId::Color, key: "color", cat: Cat::Gfx, name: "Color Converter", title: "Color Converter", desc: "Convert HEX, RGB, HSL and HSV and check contrast.", icon: "drop", keywords: "colour hex rgb hsl hsv contrast" },
    Tool { id: ToolId::Subnet, key: "subnet", cat: Cat::Net, name: "IP / Subnet", title: "IP Subnet Calculator", desc: "Work out network, mask, host range and address forms for IPv4 and IPv6.", icon: "subnet", keywords: "ip ipv4 ipv6 cidr netmask network broadcast address converter" },
    Tool { id: ToolId::IpRange, key: "iprange", cat: Cat::Net, name: "IP Ranges", title: "IP Range Expander & Summarizer", desc: "Expand ranges into addresses, or merge addresses into CIDR blocks.", icon: "range", keywords: "cidr expand merge aggregate summarize list ip range" },
    Tool { id: ToolId::Mac, key: "mac", cat: Cat::Net, name: "MAC Address", title: "MAC Address Generator", desc: "Generate random MAC addresses in common formats.", icon: "chip", keywords: "mac ethernet hardware address random" },
    Tool { id: ToolId::Ula, key: "ula", cat: Cat::Net, name: "IPv6 ULA", title: "IPv6 ULA Generator", desc: "Generate RFC 4193 unique local IPv6 prefixes.", icon: "globe", keywords: "ipv6 unique local address prefix fd00 private" },
    Tool { id: ToolId::Cron, key: "cron", cat: Cat::Test, name: "Cron", title: "Cron Expression Parser", desc: "Read cron schedules in plain English and see when they run next.", icon: "clock", keywords: "crontab schedule job timer" },
    Tool { id: ToolId::JsonPath, key: "jsonpath", cat: Cat::Test, name: "JSONPath", title: "JSONPath Playground", desc: "Query JSON documents with JSONPath expressions.", icon: "target", keywords: "query filter select jq json path" },
    Tool { id: ToolId::DataDiff, key: "datadiff", cat: Cat::Test, name: "Data Diff", title: "Structured Data Diff", desc: "Compare two JSON or YAML documents key by key.", icon: "diff", keywords: "compare json yaml changes difference" },
    Tool { id: ToolId::Regex, key: "regex", cat: Cat::Test, name: "Regular Expression", title: "Regular Expression Tester", desc: "Test patterns against sample text in real time.", icon: "regex", keywords: "regexp pattern match" },
    Tool { id: ToolId::TextCase, key: "textcase", cat: Cat::Text, name: "Text Analyzer", title: "Text Analyzer & Case Converter", desc: "Count characters and words, and change text case.", icon: "case", keywords: "camel snake kebab upper lower count words" },
    Tool { id: ToolId::Escape, key: "escape", cat: Cat::Text, name: "Escape / Unescape", title: "Text Escape / Unescape", desc: "Escape or unescape strings for JSON and code.", icon: "slash", keywords: "backslash string json" },
    Tool { id: ToolId::TextDiff, key: "textdiff", cat: Cat::Text, name: "Text Diff", title: "Text Diff Checker", desc: "Compare two texts line by line.", icon: "diff", keywords: "compare difference changes patch" },
    Tool { id: ToolId::Markdown, key: "markdown", cat: Cat::Text, name: "Markdown", title: "Markdown Preview", desc: "Write Markdown and see it rendered as you type.", icon: "md", keywords: "md readme preview render" },
    Tool { id: ToolId::Lines, key: "lines", cat: Cat::Text, name: "Sort Lines", title: "Line Sort & Dedupe", desc: "Sort, deduplicate, reverse, shuffle and clean up lines.", icon: "sort", keywords: "sort unique dedupe duplicates lines shuffle reverse natural" },
    Tool { id: ToolId::Unicode, key: "unicode", cat: Cat::Text, name: "Unicode Inspector", title: "String & Unicode Inspector", desc: "See every character's code point and bytes, and find invisible ones.", icon: "uni", keywords: "unicode utf8 utf16 code point invisible zero width character emoji" },
];

pub fn tool(id: ToolId) -> &'static Tool {
    TOOLS.iter().find(|t| t.id == id).unwrap()
}

/// A setting a tool can be opened in straight from the command palette.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Encode,
    Decode,
    ToDate,
    ToUnix,
    Minify,
}

pub struct Variant {
    pub tool: ToolId,
    pub mode: Mode,
    pub label: &'static str,
    /// Phrases in a palette query that ask for this mode, e.g. "decode" or "to date".
    pub cues: &'static [&'static str],
}

const ENCODE: &[&str] = &["encode", "encoding"];
const DECODE: &[&str] = &["decode", "decoding"];

pub const VARIANTS: &[Variant] = &[
    Variant { tool: ToolId::Base64, mode: Mode::Encode, label: "Encode", cues: ENCODE },
    Variant { tool: ToolId::Base64, mode: Mode::Decode, label: "Decode", cues: DECODE },
    Variant { tool: ToolId::Hex, mode: Mode::Encode, label: "Encode", cues: ENCODE },
    Variant { tool: ToolId::Hex, mode: Mode::Decode, label: "Decode", cues: DECODE },
    Variant { tool: ToolId::Url, mode: Mode::Encode, label: "Encode", cues: ENCODE },
    Variant { tool: ToolId::Url, mode: Mode::Decode, label: "Decode", cues: DECODE },
    Variant { tool: ToolId::Html, mode: Mode::Encode, label: "Encode", cues: &["encode", "encoding", "escape"] },
    Variant { tool: ToolId::Html, mode: Mode::Decode, label: "Decode", cues: &["decode", "decoding", "unescape"] },
    Variant { tool: ToolId::Escape, mode: Mode::Encode, label: "Escape", cues: &["escape", "encode"] },
    Variant { tool: ToolId::Escape, mode: Mode::Decode, label: "Unescape", cues: &["unescape", "decode"] },
    Variant {
        tool: ToolId::Date,
        mode: Mode::ToDate,
        label: "Timestamp → Date",
        cues: &["to date", "to human", "to readable", "to iso", "from unix", "from timestamp", "from epoch"],
    },
    Variant {
        tool: ToolId::Date,
        mode: Mode::ToUnix,
        label: "Date → Timestamp",
        cues: &["to unix", "to timestamp", "to epoch", "from date"],
    },
    Variant { tool: ToolId::JsonFmt, mode: Mode::Minify, label: "Minify", cues: &["minify", "minified", "compact"] },
];

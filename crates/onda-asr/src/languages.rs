//! Spoken languages the catalog's Whisper models accept, in whisper.cpp's token order.
//!
//! Every catalog model (tiny, base and small, ggml revision 5359861c) has 99 language tokens.
//! Large-v3 and its turbo variant add Cantonese (`yue`) as a 100th; none of the catalog models
//! has it, and whisper.cpp 1.8.3 does not check: for them `yue` becomes the `<|translate|>`
//! token, a prompt the model never saw in training. So codes outside this list are refused
//! instead of being passed through. English-only models ignore the language entirely.
//!
//! Whisper has a single `zh` code for every variety of Chinese, trained mostly on Mandarin, and
//! may write either Simplified or Traditional characters.
pub const WHISPER_LANGUAGES: [&str; 99] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su",
];

/// Whether `code` is a language token the catalog models have.
pub fn is_supported_language(code: &str) -> bool {
    WHISPER_LANGUAGES.contains(&code)
}

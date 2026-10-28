use std::path::{Path, PathBuf};

use crate::engine::state::{ensure_safe_destination_path_with_reserved, sanitize_filename};

#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub date: Option<String>,
    pub id: Option<String>,
    pub uploader: Option<String>,
    pub ext: String,
}

pub struct FilenameTemplateEngine;

impl FilenameTemplateEngine {
    /// Renders a sanitized filename stem from a template and context.
    /// Does not include the trailing extension.
    pub fn render_stem(template: Option<&str>, ctx: &TemplateContext) -> String {
        let raw_rendered = match template.map(|t| t.trim()).filter(|t| !t.is_empty()) {
            Some(t) => {
                // Strip .{ext} or {ext} from template end if user typed it
                let mut stripped = t.to_string();
                if stripped.ends_with(".{ext}") {
                    stripped.truncate(stripped.len() - 6);
                } else if stripped.ends_with("{ext}") {
                    stripped.truncate(stripped.len() - 5);
                }

                let artist_val = ctx
                    .artist
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| ctx.uploader.as_deref().filter(|s| !s.trim().is_empty()))
                    .unwrap_or("Unknown Artist");

                let album_val = ctx
                    .album
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Unknown Album");

                let date_val = ctx
                    .date
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("");

                let id_val = ctx
                    .id
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("");

                let uploader_val = ctx
                    .uploader
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| ctx.artist.as_deref().filter(|s| !s.trim().is_empty()))
                    .unwrap_or("Unknown Uploader");

                let mut out = stripped
                    .replace("{title}", &ctx.title)
                    .replace("{artist}", artist_val)
                    .replace("{album}", album_val)
                    .replace("{date}", date_val)
                    .replace("{id}", id_val)
                    .replace("{uploader}", uploader_val);

                // Clean any leftover double hyphens or spacing from empty date/id
                out = out
                    .replace(" -  - ", " - ")
                    .replace(" ()", "")
                    .trim()
                    .to_string();
                if out.is_empty() {
                    ctx.title.clone()
                } else {
                    out
                }
            }
            None => {
                // Default: "{artist} - {title}" if artist present and distinct, else "{title}"
                if let Some(ref artist) = ctx.artist {
                    let a_trim = artist.trim();
                    if !a_trim.is_empty()
                        && !ctx.title.to_lowercase().starts_with(&a_trim.to_lowercase())
                    {
                        format!("{a_trim} - {}", ctx.title.trim())
                    } else {
                        ctx.title.trim().to_string()
                    }
                } else {
                    ctx.title.trim().to_string()
                }
            }
        };

        sanitize_filename(&raw_rendered)
    }

    /// Renders a full destination path using the template and ensures collision protection.
    pub fn render_destination<P: AsRef<Path>>(
        dir: P,
        template: Option<&str>,
        ctx: &TemplateContext,
    ) -> PathBuf {
        Self::render_destination_with_reserved(dir, template, ctx, &[])
    }

    /// Renders a full destination path checking reserved in-flight paths and collisions.
    pub fn render_destination_with_reserved<P: AsRef<Path>>(
        dir: P,
        template: Option<&str>,
        ctx: &TemplateContext,
        reserved_paths: &[String],
    ) -> PathBuf {
        let stem = Self::render_stem(template, ctx);
        let ext = ctx.ext.trim_start_matches('.');
        ensure_safe_destination_path_with_reserved(dir, &stem, ext, reserved_paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_template_with_artist() {
        let ctx = TemplateContext {
            title: "Yellow".to_string(),
            artist: Some("Coldplay".to_string()),
            ext: "mp3".to_string(),
            ..TemplateContext::default()
        };
        let stem = FilenameTemplateEngine::render_stem(None, &ctx);
        assert_eq!(stem, "Coldplay - Yellow");
    }

    #[test]
    fn test_default_template_without_artist() {
        let ctx = TemplateContext {
            title: "Yellow".to_string(),
            artist: None,
            ext: "mp3".to_string(),
            ..TemplateContext::default()
        };
        let stem = FilenameTemplateEngine::render_stem(None, &ctx);
        assert_eq!(stem, "Yellow");
    }

    #[test]
    fn test_custom_template_token_replacement() {
        let ctx = TemplateContext {
            title: "Song Title".to_string(),
            artist: Some("The Artist".to_string()),
            album: Some("Greatest Hits".to_string()),
            date: Some("2021".to_string()),
            id: Some("dQw4w9WgXcQ".to_string()),
            uploader: Some("OfficialChannel".to_string()),
            ext: "flac".to_string(),
        };

        let t1 = "{artist} - {album} - {title}";
        assert_eq!(
            FilenameTemplateEngine::render_stem(Some(t1), &ctx),
            "The Artist - Greatest Hits - Song Title"
        );

        let t2 = "[{date}] {title} ({id})";
        assert_eq!(
            FilenameTemplateEngine::render_stem(Some(t2), &ctx),
            "[2021] Song Title (dQw4w9WgXcQ)"
        );
    }

    #[test]
    fn test_windows_reserved_names_protection() {
        let ctx = TemplateContext {
            title: "CON".to_string(),
            artist: None,
            ext: "mp3".to_string(),
            ..TemplateContext::default()
        };
        let stem = FilenameTemplateEngine::render_stem(Some("{title}"), &ctx);
        assert_eq!(stem, "_CON");

        let ctx_aux = TemplateContext {
            title: "aux".to_string(),
            artist: None,
            ext: "m4a".to_string(),
            ..TemplateContext::default()
        };
        let stem_aux = FilenameTemplateEngine::render_stem(Some("{title}"), &ctx_aux);
        assert_eq!(stem_aux, "_aux");
    }

    #[test]
    fn test_path_traversal_and_illegal_characters() {
        let ctx = TemplateContext {
            title: "../../../etc/passwd:*?\"<>|".to_string(),
            artist: Some("Hacker/..\\Group".to_string()),
            ext: "mp3".to_string(),
            ..TemplateContext::default()
        };
        let stem = FilenameTemplateEngine::render_stem(Some("{artist} - {title}"), &ctx);
        assert!(!stem.contains(".."));
        assert!(!stem.contains('/'));
        assert!(!stem.contains('\\'));
        assert!(!stem.contains(':'));
        assert!(!stem.contains('*'));
        assert!(!stem.contains('?'));
        assert!(!stem.contains('"'));
        assert!(!stem.contains('<'));
        assert!(!stem.contains('>'));
        assert!(!stem.contains('|'));
    }

    #[test]
    fn test_long_filename_truncation() {
        let very_long_title = "A".repeat(300);
        let ctx = TemplateContext {
            title: very_long_title,
            artist: None,
            ext: "opus".to_string(),
            ..TemplateContext::default()
        };
        let stem = FilenameTemplateEngine::render_stem(None, &ctx);
        assert!(stem.len() <= 180);
    }

    #[test]
    fn test_collision_handling() {
        let tmp = std::env::temp_dir().join(format!("ff_test_tpl_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);

        let ctx = TemplateContext {
            title: "UniqueTrack".to_string(),
            artist: Some("Band".to_string()),
            ext: "mp3".to_string(),
            ..TemplateContext::default()
        };

        let p1 = FilenameTemplateEngine::render_destination(&tmp, None, &ctx);
        assert_eq!(p1.file_name().unwrap(), "Band - UniqueTrack.mp3");
        std::fs::write(&p1, b"1").unwrap();

        let p2 = FilenameTemplateEngine::render_destination(&tmp, None, &ctx);
        assert_eq!(p2.file_name().unwrap(), "Band - UniqueTrack (1).mp3");
        std::fs::write(&p2, b"2").unwrap();

        let p3 = FilenameTemplateEngine::render_destination(&tmp, None, &ctx);
        assert_eq!(p3.file_name().unwrap(), "Band - UniqueTrack (2).mp3");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

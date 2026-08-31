struct CachedImage {
    image: Image,
    bytes: usize,
    used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ParagraphCacheKey {
    text: String,
    width: u32,
    height: u32,
    style: TextStyle,
    families: Vec<&'static str>,
}

struct CachedParagraph {
    paragraph: Paragraph,
    bytes: usize,
    used: u64,
}

pub(crate) struct SkiaCache {
    entries: HashMap<String, CachedImage>,
    paragraphs: HashMap<ParagraphCacheKey, CachedParagraph>,
    fonts: FontCollection,
    resident_bytes: usize,
    budget_bytes: usize,
    generation: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    text_hits: u64,
    text_misses: u64,
    text_evictions: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SkiaCacheStats {
    pub budget_bytes: usize,
    pub resident_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub text_resident_bytes: usize,
    pub text_entries: usize,
    pub text_hits: u64,
    pub text_misses: u64,
    pub text_evictions: u64,
    pub largest_entry_bytes: usize,
    pub largest_text_entry_bytes: usize,
}

impl SkiaCache {
    pub(crate) fn new(budget_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            paragraphs: HashMap::new(),
            fonts: skia_font_collection(),
            resident_bytes: 0,
            budget_bytes,
            generation: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            text_hits: 0,
            text_misses: 0,
            text_evictions: 0,
        }
    }

    fn draw_text(&mut self, canvas: &Canvas, rect: UiRect, text: &str, style: TextStyle) {
        let key = ParagraphCacheKey {
            text: text.to_owned(),
            width: rect.width().to_bits(),
            height: rect.height().to_bits(),
            style,
            families: crate::text::font_families().to_vec(),
        };
        if let Some(entry) = self.paragraphs.get_mut(&key) {
            self.text_hits = self.text_hits.saturating_add(1);
            entry.used = self.generation;
            paint_cached_paragraph(canvas, rect, &entry.paragraph);
            return;
        }
        self.text_misses = self.text_misses.saturating_add(1);
        let request = scene_text_layout_request(text, rect, style);
        let mut paragraph = build_skia_paragraph(
            &request,
            self.fonts.clone(),
            Some(sk_color(style.color, style.alpha)),
        );
        paragraph.layout(rect.width().max(1.0));
        paint_cached_paragraph(canvas, rect, &paragraph);
        let bytes = paragraph_cache_entry_bytes(text, &paragraph);
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.paragraphs.insert(
            key,
            CachedParagraph {
                paragraph,
                bytes,
                used: self.generation,
            },
        );
        self.evict_to_budget();
    }

    fn get(&mut self, key: &str) -> Option<Image> {
        let Some(entry) = self.entries.get_mut(key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        entry.used = self.generation;
        Some(entry.image.clone())
    }

    fn insert(&mut self, key: String, image: Image) -> Image {
        let bytes = image.width().max(0) as usize * image.height().max(0) as usize * 4;
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.bytes);
        }
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            CachedImage {
                image: image.clone(),
                bytes,
                used: self.generation,
            },
        );
        self.evict_to_budget();
        image
    }

    pub(crate) fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn evict_to_budget(&mut self) {
        while self.resident_bytes > self.budget_bytes {
            let image = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, entry)| (key.clone(), entry.used));
            let paragraph = self
                .paragraphs
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, entry)| (key.clone(), entry.used));
            if image.is_none() && paragraph.is_none() {
                break;
            }
            if paragraph.as_ref().is_some_and(|(_, paragraph_used)| {
                image
                    .as_ref()
                    .is_none_or(|(_, image_used)| paragraph_used <= image_used)
            }) {
                if let Some((key, _)) = paragraph {
                    if let Some(entry) = self.paragraphs.remove(&key) {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                        self.text_evictions = self.text_evictions.saturating_add(1);
                    }
                }
            } else if let Some((key, _)) = image {
                if let Some(entry) = self.entries.remove(&key) {
                    self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    self.evictions = self.evictions.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Moderate => {
                let current = self.generation;
                let before = self.entries.len();
                self.entries.retain(|_, entry| {
                    let keep = current.wrapping_sub(entry.used) <= 2;
                    if !keep {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    }
                    keep
                });
                self.evictions = self
                    .evictions
                    .saturating_add(before.saturating_sub(self.entries.len()) as u64);
                let before = self.paragraphs.len();
                self.paragraphs.retain(|_, entry| {
                    let keep = current.wrapping_sub(entry.used) <= 2;
                    if !keep {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    }
                    keep
                });
                self.text_evictions = self
                    .text_evictions
                    .saturating_add(before.saturating_sub(self.paragraphs.len()) as u64);
                self.fonts.clear_caches();
            }
            MemoryPressure::Critical => {
                self.evictions = self.evictions.saturating_add(self.entries.len() as u64);
                self.text_evictions = self
                    .text_evictions
                    .saturating_add(self.paragraphs.len() as u64);
                self.entries.clear();
                self.paragraphs.clear();
                self.fonts.clear_caches();
                self.resident_bytes = 0;
            }
        }
    }

    pub(crate) fn stats(&self) -> SkiaCacheStats {
        let text_resident_bytes = self.paragraphs.values().map(|entry| entry.bytes).sum();
        SkiaCacheStats {
            budget_bytes: self.budget_bytes,
            resident_bytes: self.resident_bytes,
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            text_resident_bytes,
            text_entries: self.paragraphs.len(),
            text_hits: self.text_hits,
            text_misses: self.text_misses,
            text_evictions: self.text_evictions,
            largest_entry_bytes: self
                .entries
                .values()
                .map(|entry| entry.bytes)
                .max()
                .unwrap_or(0),
            largest_text_entry_bytes: self
                .paragraphs
                .values()
                .map(|entry| entry.bytes)
                .max()
                .unwrap_or(0),
        }
    }
}

fn scene_text_layout_request<'a>(
    text: &'a str,
    rect: UiRect,
    style: TextStyle,
) -> crate::text::TextLayoutRequest<'a> {
    let mut request =
        crate::text::TextLayoutRequest::single_line(text, rect, style.height, style.weight);
    request.tracking = style.tracking;
    request.align = style.align;
    request
}

fn paint_cached_paragraph(canvas: &Canvas, rect: UiRect, paragraph: &Paragraph) {
    let y = rect.top + ((rect.height() - paragraph.height()) * 0.5).max(0.0);
    canvas.save();
    canvas.clip_rect(sk_rect(rect), None, true);
    paragraph.paint(canvas, (rect.left, y));
    canvas.restore();
}

fn paragraph_cache_entry_bytes(text: &str, paragraph: &Paragraph) -> usize {
    2048usize
        .saturating_add(text.len())
        .saturating_add(paragraph.line_number().saturating_mul(256))
}

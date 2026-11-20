use super::*;
use lgui_core::memory::{
    CacheEvent, CacheHooks, CacheUsage, CacheView, EntryId, EntryInfo, ShrinkLevel,
};

pub(super) struct CachedImage {
    id: EntryId,
    image: Image,
    bytes: usize,
    used: u64,
    retention: lgui_core::memory::RetentionClass,
    priority: lgui_core::memory::CachePriority,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ParagraphCacheKey {
    text: String,
    width: u32,
    height: u32,
    style: TextStyle,
    families: &'static [&'static str],
    rasterization: TextRasterization,
    hinting: lgui_core::text::TextHinting,
}

/// Most shaped labels kept at once.
const TEXT_ENTRY_LIMIT: usize = 448;

pub(super) struct CachedParagraph {
    id: EntryId,
    shaped: ShapedText,
    bytes: usize,
    used: u64,
    retention: lgui_core::memory::RetentionClass,
    priority: lgui_core::memory::CachePriority,
}

/// Where an [`EntryId`] lives, so an entry can be evicted without a scan.
pub(super) enum Slot {
    Image(String),
    Paragraph(Arc<ParagraphCacheKey>),
}

pub struct SkiaCache {
    pub(super) text_rendering: lgui_core::text::TextRenderingOptions,
    pub(super) entries: HashMap<String, CachedImage>,
    pub(super) paragraphs: HashMap<Arc<ParagraphCacheKey>, CachedParagraph>,
    pub(super) slots: HashMap<EntryId, Slot>,
    next_entry_id: u64,
    hooks: Option<CacheHooks>,
    seen_requests: u64,
    pub(super) fonts: FontCollection,
    pub(super) resident_bytes: usize,
    pub(super) budget_bytes: usize,
    pub(super) generation: u64,
    pub(super) hits: u64,
    pub(super) misses: u64,
    pub(super) evictions: u64,
    pub(super) text_hits: u64,
    pub(super) text_misses: u64,
    pub(super) text_evictions: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkiaCacheStats {
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
    pub(super) fn set_text_rendering(
        &mut self,
        options: lgui_core::text::TextRenderingOptions,
    ) -> bool {
        if self.text_rendering == options {
            return false;
        }
        // Raster layers bake coverage into pixels. Keep reusable shaped text;
        // its key records hinting independently of coverage curves.
        for key in self.entries.keys().cloned().collect::<Vec<_>>() {
            if self.remove_image(&key).is_some() {
                self.evictions += 1;
            }
        }
        self.text_rendering = options;
        true
    }

    pub fn new(budget_bytes: usize) -> Self {
        Self {
            text_rendering: Default::default(),
            entries: HashMap::new(),
            paragraphs: HashMap::new(),
            slots: HashMap::new(),
            next_entry_id: 1,
            hooks: None,
            seen_requests: 0,
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

    pub(super) fn draw_text(
        &mut self,
        canvas: &Canvas,
        rect: UiRect,
        text: &str,
        style: TextStyle,
    ) {
        let rasterization = TextRasterization::for_canvas(canvas, style.alpha);
        let families = if style.font_families.is_empty() {
            lgui_core::backend::font_families()
        } else {
            style.font_families
        };
        // The shaped paragraph does not depend on the text colour, so it is keyed without it and
        // the colour is applied when the glyph runs are drawn. A hover or focus colour change then
        // reuses the existing paragraph instead of shaping and keeping another copy.
        let paint_color = sk_color(style.color, style.alpha);
        let key = ParagraphCacheKey {
            text: text.to_owned(),
            width: rect.width().to_bits(),
            height: rect.height().to_bits(),
            style: TextStyle {
                color: Color(0),
                ..style
            },
            families,
            rasterization,
            hinting: if rasterization == TextRasterization::Grayscale {
                self.text_rendering.grayscale_hinting()
            } else {
                lgui_core::text::TextHinting::Slight
            },
        };
        if let Some(entry) = self.paragraphs.get_mut(&key) {
            self.text_hits = self.text_hits.saturating_add(1);
            entry.used = self.generation;
            entry.shaped.paint(canvas, rect, paint_color);
            return;
        }
        self.text_misses = self.text_misses.saturating_add(1);
        let request = scene_text_layout_request(text, rect, style);
        let mut paragraph = build_skia_paragraph_with_options(
            &request,
            self.fonts.clone(),
            Some(sk_color(Color(0), style.alpha)),
            rasterization,
            self.text_rendering,
        );
        paragraph.layout(rect.width().max(1.0));
        let shaped = ShapedText::from_paragraph(&mut paragraph, rect, style.vertical_align);
        drop(paragraph);
        shaped.paint(canvas, rect, paint_color);
        let bytes = shaped.estimated_bytes().saturating_add(text.len());
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        let id = self.allocate_id();
        let key = Arc::new(key);
        self.slots.insert(id, Slot::Paragraph(Arc::clone(&key)));
        self.paragraphs.insert(
            key,
            CachedParagraph {
                id,
                shaped,
                bytes,
                used: self.generation,
                retention: lgui_core::memory::RetentionClass::Session,
                priority: lgui_core::memory::CachePriority::Normal,
            },
        );
        self.evict_to_budget();
        self.trim_text_entries();
        self.notify(CacheEvent::Inserted(id));
    }

    /// Keeps the shaped-text cache to a bounded number of entries. Scrolling a long list shapes
    /// thousands of distinct labels, but only the ones near the viewport are drawn again, so the
    /// least recently used are dropped in batches (never one drawn this frame).
    fn trim_text_entries(&mut self) {
        if self.paragraphs.len() <= TEXT_ENTRY_LIMIT {
            return;
        }
        let keep = TEXT_ENTRY_LIMIT * 3 / 4;
        let mut used: Vec<u64> = self.paragraphs.values().map(|entry| entry.used).collect();
        let drop = used.len() - keep;
        let cutoff = *used.select_nth_unstable(drop).1;
        let current = self.generation;
        let stale: Vec<Arc<ParagraphCacheKey>> = self
            .paragraphs
            .iter()
            .filter(|(_, entry)| entry.used < cutoff && entry.used < current)
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale {
            if self.remove_paragraph(&key).is_some() {
                self.text_evictions = self.text_evictions.saturating_add(1);
            }
        }
    }

    pub(super) fn get(&mut self, key: &str) -> Option<Image> {
        let Some(entry) = self.entries.get_mut(key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        entry.used = self.generation;
        Some(entry.image.clone())
    }

    pub(super) fn insert(&mut self, key: String, image: Image) -> Image {
        self.insert_with_policy(
            key,
            image,
            lgui_core::memory::RetentionClass::Session,
            lgui_core::memory::CachePriority::Normal,
        )
    }

    pub(super) fn insert_with_policy(
        &mut self,
        key: String,
        image: Image,
        retention: lgui_core::memory::RetentionClass,
        priority: lgui_core::memory::CachePriority,
    ) -> Image {
        let bytes = image.width().max(0) as usize * image.height().max(0) as usize * 4;
        self.remove_image(&key);
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        let id = self.allocate_id();
        self.slots.insert(id, Slot::Image(key.clone()));
        self.entries.insert(
            key,
            CachedImage {
                id,
                image: image.clone(),
                bytes,
                used: self.generation,
                retention,
                priority,
            },
        );
        self.evict_to_budget();
        self.notify(CacheEvent::Inserted(id));
        image
    }

    pub fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// Connects this cache to the application's observer. Without hooks the
    /// cache only applies its own budget.
    pub fn set_hooks(&mut self, hooks: Option<CacheHooks>) {
        self.seen_requests = hooks.as_ref().map_or(0, CacheHooks::requested);
        self.hooks = hooks;
    }

    /// Called once a frame has been drawn.
    pub fn end_frame(&mut self) {
        self.notify(CacheEvent::FrameEnd);
        if let Some(requested) = self.hooks.as_ref().map(CacheHooks::requested) {
            if requested != self.seen_requests {
                self.seen_requests = requested;
                self.notify(CacheEvent::Requested);
            }
        }
    }

    fn allocate_id(&mut self) -> EntryId {
        let id = EntryId(self.next_entry_id);
        self.next_entry_id = self.next_entry_id.wrapping_add(1);
        id
    }

    fn notify(&mut self, event: CacheEvent) {
        let Some(observer) = self.hooks.as_ref().and_then(CacheHooks::observer) else {
            return;
        };
        observer.on_event(event, self);
    }

    fn remove_image(&mut self, key: &str) -> Option<CachedImage> {
        let entry = self.entries.remove(key)?;
        self.slots.remove(&entry.id);
        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
        Some(entry)
    }

    fn remove_paragraph(&mut self, key: &ParagraphCacheKey) -> Option<CachedParagraph> {
        let entry = self.paragraphs.remove(key)?;
        self.slots.remove(&entry.id);
        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
        Some(entry)
    }

    pub fn set_budget(&mut self, budget_bytes: usize) {
        self.budget_bytes = budget_bytes;
        self.evict_to_budget();
    }

    fn evict_to_budget(&mut self) {
        while self.resident_bytes > self.budget_bytes {
            let image = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| (entry.retention, entry.priority, entry.used))
                .map(|(key, entry)| (key.clone(), (entry.retention, entry.priority, entry.used)));
            let paragraph = self
                .paragraphs
                .iter()
                .min_by_key(|(_, entry)| (entry.retention, entry.priority, entry.used))
                .map(|(key, entry)| (key.clone(), (entry.retention, entry.priority, entry.used)));
            if image.is_none() && paragraph.is_none() {
                break;
            }
            if paragraph.as_ref().is_some_and(|(_, paragraph_order)| {
                image
                    .as_ref()
                    .is_none_or(|(_, image_order)| paragraph_order <= image_order)
            }) {
                if let Some((key, _)) = paragraph {
                    if self.remove_paragraph(&key).is_some() {
                        self.text_evictions = self.text_evictions.saturating_add(1);
                    }
                }
            } else if let Some((key, _)) = image {
                if self.remove_image(&key).is_some() {
                    self.evictions = self.evictions.saturating_add(1);
                }
            }
        }
    }

    pub fn trim(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Moderate => {
                self.shrink(ShrinkLevel::Moderate);
            }
            MemoryPressure::Critical => {
                self.shrink(ShrinkLevel::Critical);
            }
        }
    }

    pub fn stats(&self) -> SkiaCacheStats {
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

impl CacheView for SkiaCache {
    fn name(&self) -> &'static str {
        "skia-render"
    }

    fn usage(&self) -> CacheUsage {
        CacheUsage {
            cache_bytes: self.resident_bytes,
            cpu_bytes: self.resident_bytes,
            entries: self.entries.len() + self.paragraphs.len(),
            hits: self.hits + self.text_hits,
            misses: self.misses + self.text_misses,
            evictions: self.evictions + self.text_evictions,
            largest_entry_bytes: self
                .entries
                .values()
                .map(|entry| entry.bytes)
                .chain(self.paragraphs.values().map(|entry| entry.bytes))
                .max()
                .unwrap_or(0),
            ..CacheUsage::default()
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn visit_entries(&self, visit: &mut dyn FnMut(&EntryInfo)) {
        for entry in self.entries.values() {
            visit(&EntryInfo {
                id: entry.id,
                bytes: entry.bytes,
                last_used: entry.used,
                retention: entry.retention,
                priority: entry.priority,
            });
        }
        for entry in self.paragraphs.values() {
            visit(&EntryInfo {
                id: entry.id,
                bytes: entry.bytes,
                last_used: entry.used,
                retention: entry.retention,
                priority: entry.priority,
            });
        }
    }

    fn evict(&mut self, ids: &[EntryId]) -> usize {
        let before = self.resident_bytes;
        for id in ids {
            match self.slots.get(id) {
                Some(Slot::Image(key)) => {
                    let key = key.clone();
                    if self.remove_image(&key).is_some() {
                        self.evictions = self.evictions.saturating_add(1);
                    }
                }
                Some(Slot::Paragraph(key)) => {
                    let key = key.clone();
                    if self.remove_paragraph(&key).is_some() {
                        self.text_evictions = self.text_evictions.saturating_add(1);
                    }
                }
                None => {}
            }
        }
        before.saturating_sub(self.resident_bytes)
    }

    fn shrink(&mut self, level: ShrinkLevel) -> usize {
        let before = self.resident_bytes;
        match level {
            ShrinkLevel::Moderate => {
                let current = self.generation;
                let stale = |used: u64| current.wrapping_sub(used) > 2;
                let images: Vec<String> = self
                    .entries
                    .iter()
                    .filter(|(_, entry)| stale(entry.used))
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in images {
                    if self.remove_image(&key).is_some() {
                        self.evictions = self.evictions.saturating_add(1);
                    }
                }
                let paragraphs: Vec<Arc<ParagraphCacheKey>> = self
                    .paragraphs
                    .iter()
                    .filter(|(_, entry)| stale(entry.used))
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in paragraphs {
                    if self.remove_paragraph(&key).is_some() {
                        self.text_evictions = self.text_evictions.saturating_add(1);
                    }
                }
            }
            ShrinkLevel::Critical => {
                self.evictions = self.evictions.saturating_add(self.entries.len() as u64);
                self.text_evictions = self
                    .text_evictions
                    .saturating_add(self.paragraphs.len() as u64);
                self.entries.clear();
                self.paragraphs.clear();
                self.slots.clear();
                self.resident_bytes = 0;
            }
        }
        // Font data is not enumerable, so it is released with every shrink.
        self.fonts.clear_caches();
        before.saturating_sub(self.resident_bytes)
    }
}

pub(super) fn scene_text_layout_request<'a>(
    text: &'a str,
    rect: UiRect,
    style: TextStyle,
) -> lgui_core::text::TextLayoutRequest<'a> {
    lgui_core::text::TextLayoutRequest::for_style(text, rect, &style)
}

#[allow(dead_code)]
pub(super) fn paint_cached_paragraph(
    canvas: &Canvas,
    rect: UiRect,
    paragraph: &Paragraph,
    vertical_align: lgui_core::text::TextVerticalAlign,
) {
    let y = rect.top + vertical_align_offset(paragraph, rect, vertical_align);
    canvas.save();
    canvas.clip_rect(sk_rect(rect), None, true);
    paragraph.paint(canvas, (rect.left, y));
    canvas.restore();
}

/// One run of positioned glyphs of a shaped label.
pub(super) struct ShapedRun {
    glyphs: Vec<u16>,
    positions: Vec<skia_safe::Point>,
    font: skia_safe::Font,
    origin: skia_safe::Point,
}

/// What painting a label needs from its shaped paragraph. A Skia `Paragraph` keeps its whole
/// layout alive (tens of KB for a short label); the glyph runs are a few hundred bytes, so only
/// they are cached and the paragraph is dropped right after it is first painted.
pub(super) struct ShapedText {
    /// Vertical offset from the top of the text rect, resolved once for the cached rect and align.
    y_offset: f32,
    runs: Vec<ShapedRun>,
}

impl ShapedText {
    fn from_paragraph(
        paragraph: &mut Paragraph,
        rect: UiRect,
        vertical_align: lgui_core::text::TextVerticalAlign,
    ) -> Self {
        let y_offset = vertical_align_offset(paragraph, rect, vertical_align);
        let mut runs = Vec::new();
        paragraph.visit(|_line, info| {
            let Some(info) = info else {
                return;
            };
            runs.push(ShapedRun {
                glyphs: info.glyphs().to_vec(),
                positions: info.positions().to_vec(),
                font: info.font().clone(),
                origin: info.origin(),
            });
        });
        Self { y_offset, runs }
    }

    fn estimated_bytes(&self) -> usize {
        self.runs.iter().fold(256, |bytes, run| {
            bytes
                .saturating_add(run.glyphs.len().saturating_mul(2))
                .saturating_add(
                    run.positions
                        .len()
                        .saturating_mul(std::mem::size_of::<skia_safe::Point>()),
                )
                .saturating_add(128)
        })
    }

    /// Draws the glyph runs in `color`, at the same place `Paragraph::paint` would.
    fn paint(&self, canvas: &Canvas, rect: UiRect, color: SkColor) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(color);
        canvas.save();
        canvas.clip_rect(sk_rect(rect), None, true);
        for run in &self.runs {
            canvas.draw_glyphs_at(
                &run.glyphs,
                run.positions.as_slice(),
                (
                    rect.left + run.origin.x,
                    rect.top + self.y_offset + run.origin.y,
                ),
                &run.font,
                &paint,
            );
        }
        canvas.restore();
    }
}

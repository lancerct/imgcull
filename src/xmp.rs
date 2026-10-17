//! Namespace-aware, byte-preserving XMP edits.
use crate::scoring::ScoringResult;
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use quick_xml::escape::escape;
use roxmltree::{Document, Node};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const IC: &str = "http://imgcull.dev/ns/1.0/";
const EMPTY: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about=""/></rdf:RDF></x:xmpmeta>"#;
pub use crate::discovery::sidecar_for_image;
fn description_node<'a>(doc: &'a Document<'a>) -> Result<Node<'a, 'a>> {
    let nodes: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.has_tag_name((RDF, "Description"))
                && n.parent().is_some_and(|p| p.has_tag_name((RDF, "RDF")))
        })
        .collect();
    ensure!(
        nodes.len() == 1,
        "XMP must have exactly one top-level RDF Description; refusing ambiguous edit"
    );
    Ok(nodes[0])
}
fn property(node: Node<'_, '_>, ns: &str, name: &str) -> Result<Option<String>> {
    let attrs: Vec<_> = node
        .attributes()
        .filter(|a| a.namespace() == Some(ns) && a.name() == name)
        .collect();
    let children: Vec<_> = node
        .children()
        .filter(|n| n.has_tag_name((ns, name)))
        .collect();
    ensure!(
        attrs.len() + children.len() <= 1,
        "duplicate XMP property: {name}"
    );
    Ok(attrs.first().map(|a| a.value().to_string()).or_else(|| {
        children.first().map(|n| {
            n.descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect()
        })
    }))
}
#[derive(Debug, Clone, Default)]
pub struct XmpSidecar {
    description: Option<String>,
    rating: Option<u8>,
    rating_present: bool,
    label: Option<String>,
    overall_score: Option<f64>,
    dimension_scores: Vec<(String, f64)>,
    scored_at: Option<String>,
    scored_by: Option<String>,
    dimensions_list: Option<String>,
    original_filename: Option<String>,
    scoring_response: Option<String>,
    keywords: Vec<String>,
    dirty: BTreeSet<String>,
    raw_content: Option<String>,
}
impl XmpSidecar {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn read(path: &Path) -> Result<Self> {
        let content =
            fs::read_to_string(path).with_context(|| format!("reading XMP: {}", path.display()))?;
        let doc = Document::parse(&content).context("malformed XMP")?;
        let node = description_node(&doc)?;
        let rating_text = property(node, XMP, "Rating")?;
        let mut s = Self {
            description: property(node, DC, "description")?,
            rating: rating_text.as_ref().and_then(|r| r.parse().ok()),
            rating_present: rating_text.is_some(),
            label: property(node, XMP, "Label")?,
            overall_score: property(node, IC, "score")?.and_then(|v| v.parse().ok()),
            scored_at: property(node, IC, "scored_at")?,
            scored_by: property(node, IC, "scored_by")?,
            dimensions_list: property(node, IC, "dimensions")?,
            original_filename: property(node, IC, "original_filename")?,
            scoring_response: property(node, IC, "scoring_response")?,
            ..Self::default()
        };
        if let Some(desc) = node
            .children()
            .find(|n| n.has_tag_name((DC, "description")))
            && let Some(li) = desc.descendants().find(|n| {
                n.has_tag_name((RDF, "li"))
                    && n.attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
                        == Some("x-default")
            })
        {
            s.description = Some(li.text().unwrap_or("").to_string());
        }
        if let Some(subject) = node.children().find(|n| n.has_tag_name((DC, "subject"))) {
            property(node, DC, "subject")?;
            s.keywords = subject
                .descendants()
                .filter(|n| n.has_tag_name((RDF, "li")))
                .filter_map(|n| n.text())
                .map(str::to_string)
                .collect();
        }
        for name in crate::scoring::DIMENSIONS {
            if let Some(v) = property(node, IC, name)?.and_then(|v| v.parse().ok()) {
                s.dimension_scores.push((name.to_string(), v));
            }
        }
        s.raw_content = Some(content);
        Ok(s)
    }
    pub fn has_description(&self) -> bool {
        self.description.is_some()
    }
    pub fn has_scores(&self) -> bool {
        self.overall_score
            .is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))
            && crate::scoring::DIMENSIONS.iter().all(|d| {
                self.dimension_scores
                    .iter()
                    .any(|(name, v)| name == d && v.is_finite() && (0.0..=1.0).contains(v))
            })
    }
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    pub fn rating(&self) -> Option<u8> {
        self.rating
    }
    /// Includes reject ratings and unfamiliar values; automation must preserve them.
    pub fn has_rating(&self) -> bool {
        self.rating_present
    }
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }
    pub fn overall_score(&self) -> Option<f64> {
        self.overall_score
    }
    pub fn dimension_scores(&self) -> &[(String, f64)] {
        &self.dimension_scores
    }
    pub fn scored_by(&self) -> Option<&str> {
        self.scored_by.as_deref()
    }
    pub fn scored_at(&self) -> Option<&str> {
        self.scored_at.as_deref()
    }
    pub fn scoring_response(&self) -> Option<&str> {
        self.scoring_response.as_deref()
    }
    pub fn original_filename(&self) -> Option<&str> {
        self.original_filename.as_deref()
    }
    pub fn set_description(&mut self, v: &str) {
        self.description = Some(v.into());
        self.dirty.insert("description".into());
    }
    pub fn set_rating(&mut self, v: u8) {
        self.rating = Some(v);
        self.rating_present = true;
        self.dirty.insert("Rating".into());
    }
    pub fn set_label(&mut self, v: &str) {
        self.label = Some(v.into());
        self.dirty.insert("Label".into());
    }
    pub fn set_original_filename(&mut self, v: &str) {
        if self.original_filename.as_deref() != Some(v) {
            self.original_filename = Some(v.into());
            self.dirty.insert("original_filename".into());
        }
    }
    pub fn set_scoring_response(&mut self, v: &str) {
        self.scoring_response = Some(v.into());
        self.dirty.insert("scoring_response".into());
    }
    pub fn set_keywords(&mut self, v: &[String]) {
        self.keywords = v.to_vec();
        self.dirty.insert("subject".into());
    }
    pub fn set_scores(&mut self, scores: &ScoringResult, overall: f64, model: &str) {
        self.overall_score = Some(overall);
        self.scored_by = Some(model.into());
        self.scored_at = Some(Utc::now().to_rfc3339());
        self.dimensions_list = Some(crate::scoring::DIMENSIONS.join(","));
        self.dimension_scores = crate::scoring::DIMENSIONS
            .iter()
            .filter_map(|d| scores.get(d).map(|v| (d.to_string(), v)))
            .collect();
        for name in ["score", "scored_by", "scored_at", "dimensions"]
            .into_iter()
            .chain(crate::scoring::DIMENSIONS.iter().copied())
        {
            self.dirty.insert(name.into());
        }
    }
    fn field(&self, name: &str) -> (String, String) {
        let (prefix, ns) = match name {
            "description" | "subject" => ("dc", DC),
            "Rating" | "Label" => ("xmp", XMP),
            _ => ("imgcull", IC),
        };
        let value = match name {
            "description" => format!(
                "<rdf:Alt xmlns:rdf=\"{RDF}\"><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>",
                escape(self.description.as_deref().unwrap_or(""))
            ),
            "subject" => format!(
                "<rdf:Bag xmlns:rdf=\"{RDF}\">{}</rdf:Bag>",
                self.keywords
                    .iter()
                    .map(|k| format!("<rdf:li>{}</rdf:li>", escape(k)))
                    .collect::<String>()
            ),
            "Rating" => self.rating.unwrap_or(0).to_string(),
            "Label" => escape(self.label.as_deref().unwrap_or("")).into_owned(),
            "score" => format!("{:.2}", self.overall_score.unwrap_or(0.0)),
            "scored_at" => escape(self.scored_at.as_deref().unwrap_or("")).into_owned(),
            "scored_by" => escape(self.scored_by.as_deref().unwrap_or("")).into_owned(),
            "dimensions" => escape(self.dimensions_list.as_deref().unwrap_or("")).into_owned(),
            "original_filename" => {
                escape(self.original_filename.as_deref().unwrap_or("")).into_owned()
            }
            "scoring_response" => {
                escape(self.scoring_response.as_deref().unwrap_or("")).into_owned()
            }
            _ => self
                .dimension_scores
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| format!("{v:.2}"))
                .unwrap_or_default(),
        };
        (
            ns.into(),
            format!("<{prefix}:{name} xmlns:{prefix}=\"{ns}\">{value}</{prefix}:{name}>"),
        )
    }
    /// Only explicitly modified properties change; all other bytes survive unchanged.
    pub fn to_xml(&self) -> Result<String> {
        if self.dirty.contains("score") {
            ensure!(self.has_scores(), "refusing incomplete or invalid scores");
        }
        let base = self.raw_content.as_deref().unwrap_or(EMPTY);
        if self.dirty.is_empty() {
            return Ok(base.to_string());
        }
        let doc = Document::parse(base)?;
        let node = description_node(&doc)?;
        let mut edits = Vec::new();
        let mut fields = String::new();
        for name in &self.dirty {
            let (ns, fragment) = self.field(name);
            property(node, &ns, name)?;
            for a in node
                .attributes()
                .filter(|a| a.namespace() == Some(ns.as_str()) && a.name() == name)
            {
                edits.push((a.range(), String::new()));
            }
            for n in node
                .children()
                .filter(|n| n.has_tag_name((ns.as_str(), name.as_str())))
            {
                edits.push((n.range(), String::new()));
            }
            fields.push_str(&fragment);
        }
        let range = node.range();
        let raw_node = &base[range.clone()];
        if raw_node.ends_with("/>") {
            let qname = raw_node[1..]
                .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .next()
                .unwrap();
            edits.push((range.end - 2..range.end, format!(">{fields}</{qname}>")));
        } else {
            let pos = range.start + raw_node.rfind("</").context("missing Description end")?;
            edits.push((pos..pos, fields));
        }
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut xml = base.to_string();
        for (range, value) in edits {
            xml.replace_range(range, &value);
        }
        let check = Document::parse(&xml).context("generated XMP failed validation")?;
        let target = description_node(&check)?;
        for name in &self.dirty {
            let (ns, _) = self.field(name);
            ensure!(
                property(target, &ns, name)?.is_some(),
                "generated XMP property missing"
            );
        }
        if self.dirty.contains("Rating") {
            ensure!(self.rating.is_some_and(|r| r <= 5), "rating must be 0..=5");
        }
        Ok(xml)
    }
    pub fn write(&self, path: &Path) -> Result<()> {
        let xml = self.to_xml()?;
        crate::metadata_transaction::write(
            path,
            self.raw_content.as_deref().map(str::as_bytes),
            xml.as_bytes(),
        )
    }
}
/// Keep the first legacy backup immutable. Transaction snapshots preserve later versions.
pub fn backup_sidecar(path: &Path) -> Result<()> {
    use std::io::Write;
    let backup = path.with_extension("xmp.bak");
    let bytes = fs::read(path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    output.write_all(&bytes)?;
    output.as_file().sync_all()?;
    match output.persist_noclobber(backup) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.error.into()),
    }
}

//! Optional inert evidence files. They never become simulation or renderer code.
use crate::{energy::EnergyProject, persistence::*, Project, ProjectView};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyAttachment {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub media_type: String,
    pub model_input_sha256: String,
    pub caption: String,
    pub rights: String,
    /// Selection is document metadata; changing it never rewrites sealed bytes.
    pub include_in_export: bool,
}
impl EnergyAttachment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            Uuid::parse_str(&self.id)?.to_string() == self.id,
            "Attachment requires a canonical UUID"
        );
        let ext = match self.media_type.as_str() {
            "application/pdf" => "pdf",
            "image/png" => "png",
            "image/jpeg" => "jpg",
            _ => anyhow::bail!("Only PDF, PNG and JPEG evidence is supported"),
        };
        ensure!(
            self.path == format!("attachments/{}/evidence.{ext}", self.id),
            "Attachment path differs from its identity/type"
        );
        for hash in [&self.sha256, &self.model_input_sha256] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid attachment/model checksum"
            );
        }
        for text in [&self.caption, &self.rights] {
            ensure!(
                !text.trim().is_empty() && text.len() <= 16000 && !text.contains('\0'),
                "Attachment caption and source/rights declaration are required and bounded"
            );
        }
        Ok(())
    }
}
fn sniff(path: &Path) -> Result<&'static str> {
    let mut file = fs::File::open(path)?;
    let mut header = [0; 8];
    file.read_exact(&mut header)?;
    if header.starts_with(b"%PDF-") {
        Ok("application/pdf")
    } else if header == *b"\x89PNG\r\n\x1a\n" {
        Ok("image/png")
    } else if header.starts_with(&[0xff, 0xd8, 0xff]) {
        Ok("image/jpeg")
    } else {
        anyhow::bail!("Attachment is not recognized PDF/PNG/JPEG data")
    }
}
pub fn verify(root: &Path, item: &EnergyAttachment) -> Result<()> {
    item.validate()?;
    let directory = root.join("attachments").join(&item.id);
    let sums = verify_seal(&directory)?;
    let leaf = item.path.rsplit('/').next().unwrap();
    ensure!(
        sums.sha256.len() == 2
            && sums.sha256.get(leaf) == Some(&item.sha256)
            && sums.sha256.contains_key("attachment.json"),
        "Attachment seal coverage differs"
    );
    let mut expected = item.clone();
    expected.include_in_export = false;
    let retained: EnergyAttachment = read_json(&regular_child(&directory, "attachment.json")?)?;
    ensure!(
        retained == expected && sniff(&regular_child(root, &item.path)?)? == item.media_type,
        "Attachment identity/type differs from retained evidence"
    );
    Ok(())
}
pub fn copy(source: &Path, destination: &Path, item: &EnergyAttachment) -> Result<()> {
    verify(source, item)?;
    promote_attachment(
        &source.join("attachments").join(&item.id),
        destination,
        &item.id,
    )?;
    verify(destination, item)
}
pub fn add(
    root: &Path,
    project: &Project,
    revision: Option<&str>,
    alternative: &str,
    file: &Path,
    caption: String,
    rights: String,
) -> Result<ProjectView> {
    let _lock = WriterLock::acquire(root)?;
    check_revision(root, revision)?;
    let extension = project.energy.as_ref().context("Energy project missing")?;
    let design = extension
        .designs
        .get(alternative)
        .context("Select A or B for the illustrated model")?;
    let model = &design.model;
    let calendar = design
        .study
        .as_ref()
        .map(|s| s.calendar.clone())
        .unwrap_or_else(|| greenlens_core::energy::EnergyCalendar {
            start_weekday: greenlens_core::energy::Weekday::Monday,
            utc_offset_minutes: 0,
            holidays: model
                .schedules
                .iter()
                .flat_map(|s| s.holidays.iter().map(|h| h.date))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        });
    let model_hash =
        greenlens_core::energy::validate_thermal_model(model, &calendar)?.model_input_sha256;
    let parent = file.parent().context("Attachment parent missing")?;
    let leaf = file
        .file_name()
        .and_then(|s| s.to_str())
        .context("Invalid attachment filename")?;
    let source = regular_child(parent, leaf)?;
    ensure!(
        fs::metadata(&source)?.len() <= MAX_EVIDENCE_FILE_BYTES,
        "Attachment exceeds64MiB; reduce its size before import"
    );
    let media = sniff(&source)?;
    let ext = match media {
        "application/pdf" => "pdf",
        "image/png" => "png",
        _ => "jpg",
    };
    let id = Uuid::new_v4().to_string();
    let stage = crate::local_root()?.join("energy-attachments").join(&id);
    fs::create_dir_all(&stage)?;
    let staged = stage.join(format!("evidence.{ext}"));
    let mut input = fs::File::open(source)?;
    ensure!(
        input.metadata()?.is_file(),
        "Attachment must remain a regular file"
    );
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)?;
    copy_bounded(&mut input, &mut output, MAX_EVIDENCE_FILE_BYTES)?;
    output.sync_all()?;
    drop(output);
    ensure!(
        fs::metadata(&staged)?.len() <= MAX_EVIDENCE_FILE_BYTES && sniff(&staged)? == media,
        "Attachment changed during import"
    );
    let item = EnergyAttachment {
        id: id.clone(),
        path: format!("attachments/{id}/evidence.{ext}"),
        sha256: greenlens_process::file_hash(&staged)?,
        media_type: media.into(),
        model_input_sha256: model_hash,
        caption,
        rights,
        include_in_export: false,
    };
    item.validate()?;
    write_json(&stage.join("attachment.json"), &item)?;
    seal(&stage, &[&format!("evidence.{ext}"), "attachment.json"])?;
    promote_attachment(&stage, root, &id)?;
    let mut next = project.clone();
    next.energy.as_mut().unwrap().attachments.push(item);
    crate::validate_project(&next)?;
    check_revision(root, revision)?;
    atomic_json(root, &next)?;
    crate::open_project(root)
}
pub fn set_inclusion(project: &Project, id: &str, included: bool) -> Result<Project> {
    let mut next = project.clone();
    next.energy
        .as_mut()
        .context("Energy project missing")?
        .attachments
        .iter_mut()
        .find(|a| a.id == id)
        .context("Attachment missing")?
        .include_in_export = included;
    crate::validate_project(&next)?;
    Ok(next)
}
pub fn report_evidence(
    project: &EnergyProject,
    model_a: &str,
    model_b: &str,
) -> Vec<greenlens_core::energy::EnergyEvidence> {
    project
        .attachments
        .iter()
        .filter(|a| a.model_input_sha256 == model_a || a.model_input_sha256 == model_b)
        .map(|a| greenlens_core::energy::EnergyEvidence {
            id: format!("attachment_{}", a.id.replace('-', "")),
            relative_path: a.path.clone(),
            sha256: a.sha256.clone(),
            caption: a.caption.clone(),
            included: a.include_in_export,
            rights: a.rights.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inert_attachment_is_private_until_selected_and_copies_without_source() {
        let root = tempfile::tempdir().unwrap();
        let view = crate::save_project(
            root.path(),
            &crate::energy_services::example_document(),
            None,
        )
        .unwrap();
        let input = tempfile::tempdir().unwrap();
        let file = input.path().join("original.pdf");
        fs::write(
            &file,
            b"%PDF-1.7\nSynthetic inert import fixture, not a rendered PDF\n",
        )
        .unwrap();
        let view = add(
            root.path(),
            &view.project,
            Some(&view.revision),
            "A",
            &file,
            "Synthetic evidence".into(),
            "Original fixture; Apache-2.0".into(),
        )
        .unwrap();
        let item = &view.project.energy.as_ref().unwrap().attachments[0];
        assert!(!item.include_in_export);
        let bytes = fs::read(root.path().join(&item.path)).unwrap();
        fs::remove_file(file).unwrap();
        verify(root.path(), item).unwrap();
        let next = set_inclusion(&view.project, &item.id, true).unwrap();
        let item = &next.energy.as_ref().unwrap().attachments[0];
        verify(root.path(), item).unwrap();
        let other = tempfile::tempdir().unwrap();
        copy(root.path(), other.path(), item).unwrap();
        assert_eq!(fs::read(other.path().join(&item.path)).unwrap(), bytes);
        fs::write(other.path().join(&item.path), b"changed").unwrap();
        assert!(verify(other.path(), item).is_err());
        let mut unsafe_item = item.clone();
        unsafe_item.path = "../private.pdf".into();
        assert!(unsafe_item.validate().is_err());
    }
}

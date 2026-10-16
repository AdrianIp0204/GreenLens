//! Concurrent read reservations, bounded independently from document replacement.
use crate::{blocking, error, CommandResult};
use greenlens_runner::{energy_inspection::EnergyEvidenceQuery, project_open::OpenProgress};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, Manager};

const MAX_INSPECTIONS: usize = 32;
type InspectionReservation = (Arc<AtomicBool>, bool, Instant);
#[derive(Default)]
pub struct EvidenceControl {
    next: AtomicU64,
    active: Mutex<BTreeMap<u64, InspectionReservation>>,
}
impl EvidenceControl {
    fn reserve(&self) -> CommandResult<u64> {
        let mut active = self.active.lock().map_err(error)?;
        active.retain(|_, (_, started, reserved)| {
            *started || reserved.elapsed() < Duration::from_secs(30)
        });
        if active.len() >= MAX_INSPECTIONS {
            return Err(
                "Evidence inspection capacity reached; wait for or cancel a running inspection."
                    .into(),
            );
        }
        let ticket = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        active.insert(
            ticket,
            (Arc::new(AtomicBool::new(false)), false, Instant::now()),
        );
        Ok(ticket)
    }
    pub(crate) fn start(&self, ticket: u64) -> CommandResult<Arc<AtomicBool>> {
        let mut active = self.active.lock().map_err(error)?;
        let (cancel, started, reserved) = active
            .get_mut(&ticket)
            .ok_or("Evidence inspection reservation expired")?;
        if !*started && reserved.elapsed() >= Duration::from_secs(30) {
            active.remove(&ticket);
            return Err("Evidence inspection reservation expired; retry the query.".into());
        }
        if *started {
            return Err("Evidence inspection already started".into());
        }
        *started = true;
        Ok(cancel.clone())
    }
    fn cancel(&self, ticket: u64) -> CommandResult<()> {
        let mut active = self.active.lock().map_err(error)?;
        if let Some((cancel, started, _)) = active.get(&ticket) {
            cancel.store(true, Ordering::Relaxed);
            if !started {
                active.remove(&ticket);
            }
        }
        Ok(())
    }
    pub(crate) fn finish(&self, ticket: u64) -> CommandResult<()> {
        self.active.lock().map_err(error)?.remove(&ticket);
        Ok(())
    }
    pub fn cancel_all(&self) {
        if let Ok(mut active) = self.active.lock() {
            active.retain(|_, (cancel, started, _)| {
                cancel.store(true, Ordering::Relaxed);
                *started
            });
        }
    }
}
#[tauri::command]
pub fn prepare_evidence_inspection(control: tauri::State<EvidenceControl>) -> CommandResult<u64> {
    control.reserve()
}
#[tauri::command]
pub fn cancel_evidence_inspection(
    control: tauri::State<EvidenceControl>,
    ticket: u64,
) -> CommandResult<()> {
    control.cancel(ticket)
}
#[tauri::command]
pub async fn inspect_energy_evidence(
    app: tauri::AppHandle,
    ticket: u64,
    query: EnergyEvidenceQuery,
    on_progress: Channel<OpenProgress>,
) -> CommandResult<serde_json::Value> {
    let cancel = app.state::<EvidenceControl>().start(ticket)?;
    let result = blocking(move || {
        query.execute(cancel, move |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await;
    app.state::<EvidenceControl>().finish(ticket)?;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unstarted_expired_reservations_are_reclaimed_and_cannot_restart() {
        let control = EvidenceControl::default();
        let old = control.reserve().unwrap();
        control.active.lock().unwrap().get_mut(&old).unwrap().2 =
            Instant::now() - Duration::from_secs(31);
        assert!(control.start(old).unwrap_err().contains("expired"));
        for _ in 0..MAX_INSPECTIONS {
            control.reserve().unwrap();
        }
        for (_, _, reserved) in control.active.lock().unwrap().values_mut() {
            *reserved = Instant::now() - Duration::from_secs(31)
        }
        assert!(control.reserve().is_ok());
        assert_eq!(control.active.lock().unwrap().len(), 1);
    }
    #[test]
    fn reservations_are_bounded_and_cancellation_does_not_release_running_slots() {
        let control = EvidenceControl::default();
        let mut tickets = vec![];
        for _ in 0..MAX_INSPECTIONS {
            tickets.push(control.reserve().unwrap())
        }
        assert!(control.reserve().is_err());
        let token = control.start(tickets[0]).unwrap();
        control.cancel(tickets[0]).unwrap();
        assert!(token.load(Ordering::Relaxed));
        assert!(control.reserve().is_err());
        control.finish(tickets[0]).unwrap();
        assert!(control.reserve().is_ok());
        control.cancel(tickets[1]).unwrap();
        assert!(control.start(tickets[1]).is_err());
        control.cancel_all();
        assert_eq!(control.active.lock().unwrap().len(), 0);
    }
}

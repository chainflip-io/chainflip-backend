use crate::local_store::LocalEvent;

/// Wraps an event with its event number for easy sorting and filtering on event_number
pub struct EventNumberLocalEvent {
    /// Event number from the database
    pub event_number: Option<u64>,
    /// the LocalEvent being wrapped
    pub local_event: LocalEvent,
}

impl From<&LocalEvent> for EventNumberLocalEvent {
    fn from(event: &LocalEvent) -> Self {
        match event {
            LocalEvent::Withdraw(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::Witness(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::DepositQuote(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::Deposit(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::OutputSent(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::Output(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::PoolChange(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::SwapQuote(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
            LocalEvent::WithdrawRequest(e) => EventNumberLocalEvent {
                event_number: e.event_number,
                local_event: event.clone(),
            },
        }
    }
}

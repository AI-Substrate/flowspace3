# hidden-dirs stop-and-ask 001

After delivering the ack, I ran the packet's required start status-card command exactly as written: `pij report now`.

Result: exit 4, `E-RS`, with usage requiring `pij report now "<did>" "<next>" [--state <word>]`; it explicitly did not fall back to legacy.

Please rule the intended `did` / `next` values or confirm that the ack pointer satisfies the start edge. No code has changed; implementation remains gated on the ack ruling.

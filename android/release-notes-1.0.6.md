# SNI Tunnel Android and Android TV 1.0.6

A partial CPU wake lock keeps the foreground VPN service working while the
display is off. The same lifecycle applies to phone and TV: connection setup,
active forwarding and recovery are covered, then Disconnect, startup failure,
VPN revocation or service destruction releases the lock.

Signed updates retain existing application IDs, signing identity and saved profiles.
Version code: 7. Desktop builds are unchanged.

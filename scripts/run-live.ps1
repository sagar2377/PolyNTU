# Live demonstration run: the same pipeline as run-prod.ps1 with the bus
# provider authoritative, so bus brackets resolve from the campus Omnibus feed
# and void at their deadline rather than falling back to the simulator.
& (Join-Path $PSScriptRoot 'run-prod.ps1') -Live

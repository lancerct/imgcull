-- A runtime UI flag, not a second review/job ledger. Reset after LR/plugin restart.
local prefs = import 'LrPrefs'.prefsForPlugin()
prefs.busy = false

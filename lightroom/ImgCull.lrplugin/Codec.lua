-- Deterministic settings fingerprints, using the pinned JSON implementation
-- for scalar encoding/escaping. No second JSON parser or custom crypto.
local LrPathUtils = import 'LrPathUtils'
local json = dofile(LrPathUtils.child(_PLUGIN.path, 'vendor/json.lua'))
local M = { json = json }

function M.canonical(value, active)
    if type(value) ~= 'table' then return json.encode(value) end
    active = active or {}
    assert(not active[value], 'Cyclic develop settings cannot be fingerprinted')
    active[value] = true
    local keys, numeric, strings = {}, 0, 0
    for key in pairs(value) do
        assert(type(key) == 'string' or type(key) == 'number', 'Unsupported settings key')
        keys[#keys + 1] = key
        if type(key) == 'number' then numeric = numeric + 1 else strings = strings + 1 end
    end
    assert(numeric == 0 or strings == 0, 'Mixed settings table cannot be fingerprinted')
    table.sort(keys)
    local parts = {}
    for index, key in ipairs(keys) do
        if numeric > 0 then
            assert(key == index, 'Sparse settings array cannot be fingerprinted')
            parts[index] = M.canonical(value[key], active)
        else
            parts[index] = json.encode(key) .. ':' .. M.canonical(value[key], active)
        end
    end
    active[value] = nil
    if numeric > 0 then return '[' .. table.concat(parts, ',') .. ']' end
    return '{' .. table.concat(parts, ',') .. '}'
end

return M

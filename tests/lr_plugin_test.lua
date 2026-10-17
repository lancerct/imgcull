-- SDK orchestration tests, not a simulation of Adobe's image renderer.
local repo = assert(arg[1], 'pass repository root')
_PLUGIN = { path = repo .. '/lightroom/ImgCull.lrplugin' }
local files, dirs, failMove, oversized, hook, cancel = {}, {}, false, false, nil, false
local counter, tests = 0, 0
local function read(path)
    if files[path] then return files[path] end
    local f = assert(io.open(path, 'rb')); local bytes = f:read('*a'); f:close(); return bytes
end
local function digest(bytes)
    -- Exercise real SHA-256 compatibility without reimplementing cryptography.
    local temporary = os.tmpname()
    assert(temporary:match('^[/%w_%.%-]+$'))
    local f = assert(io.open(temporary, 'wb')); assert(f:write(bytes)); assert(f:close())
    local process = assert(io.popen('sha256sum ' .. string.format('%q', temporary)))
    local value = assert(process:read('*l')):match('^(%x+)')
    assert(process:close()); os.remove(temporary)
    return assert(value)
end
local File = {
    exists = function(path) return files[path] and 'file' or dirs[path] and 'directory' or false end,
    fileAttributes = function(path) return { fileSize = oversized and 600 * 1024 * 1024 or #read(path) } end,
    readFile = read,
    createDirectory = function(path) assert(not files[path] and not dirs[path]); dirs[path] = true end,
    chooseUniqueFileName = function(path)
        local candidate, serial = path, 0
        while files[candidate] or dirs[candidate] do serial = serial + 1; candidate = path .. '-' .. serial end
        return candidate
    end,
    move = function(source, target)
        if failMove or files[target] or dirs[target] then return false, 'mock failure/collision' end
        files[target] = read(source)
        if files[source] then files[source] = nil else os.remove(source) end
        return true
    end,
    delete = function(path) files[path] = nil; os.remove(path); return true end,
}
local Path = {
    child = function(a, b) return a .. '/' .. b end,
    parent = function(path) return path:match('^(.*)/[^/]+$') end,
    standardizePath = function(path) return path end,
    getStandardFilePath = function() return '/tmp' end,
}
local currentBytes = read(repo .. '/tests/fixtures/lr-current.jpg')
local sourceBytes = read(repo .. '/tests/fixtures/lr-source.jpg')
local optionsSeen = {}
local function Session(options)
    optionsSeen[#optionsSeen + 1] = options.exportSettings
    local photo = options.photosToExport[1]
    return { renditions = function()
        local used = false
        return function()
            if used then return nil end
            used = true
            return 1, { waitForRender = function()
                local output = options.exportSettings.LR_export_destinationPathPrefix .. '/camera-name.JPG'
                files[output] = currentBytes
                if hook then hook(photo) end
                return true, output
            end }
        end
    end }
end
local namespaces = { LrFileUtils = File, LrPathUtils = Path,
    LrDigest = { SHA256 = { digest = digest } }, LrExportSession = Session,
    LrDate = { currentTime = function() counter = counter + 1; return os.time() + counter / 1000 end },
    LrApplication = { versionString = function() return 'SDK-mock' end },
}
function import(name) return assert(namespaces[name], 'Unexpected SDK import: ' .. name) end
local Exporter = dofile(_PLUGIN.path .. '/Exporter.lua')
local Codec = dofile(_PLUGIN.path .. '/Codec.lua')
local catalog = { getPath = function() return repo .. '/target/synthetic.lrcat' end }
local progress = { isCanceled = function() return cancel end, setPortionComplete = function() end }
local function photo(id, source)
    files[source] = sourceBytes
    local value = { localIdentifier = id, settings = { Exposure2012 = 0.5, CropLeft = 0.1 },
        metadata = { path = source, uuid = 'photo-' .. id, isVirtualCopy = false,
            lastEditTime = 100, editCount = 2, croppedDimensions = { width = 40, height = 30 } } }
    function value:getDevelopSettings() return self.settings end
    function value:getRawMetadata(key) return self.metadata[key] end
    function value:checkPhotoAvailability() return not self.offline end
    return value
end
local serial = 0
local function fresh()
    serial = serial + 1; cancel = false; hook = nil; failMove = false; oversized = false
    return photo(serial, '/synthetic/source-' .. serial .. '.JPG'), '/synthetic/output-' .. serial
end
local function test(name, body)
    body(); tests = tests + 1; print('PASS ' .. name)
end
local function blocked(p, root)
    assert(not pcall(Exporter.run, { p }, catalog, root, progress))
    assert(not files[root .. '/manifest.json'])
end

test('full export keeps original bytes, disables resize/sharpen/watermark/reimport', function()
    local p, root = fresh(); local source = files[p.metadata.path]
    local manifest = Exporter.run({ p }, catalog, root, progress)
    assert(files[root .. '/manifest.json'] and files[root .. '/render-0001.jpg'])
    assert(files[p.metadata.path] == source and manifest.images[1].source_sha256 == digest(source))
    local settings = optionsSeen[#optionsSeen]
    assert(settings.LR_size_doConstrain == false and settings.LR_outputSharpeningOn == false)
    assert(settings.LR_useWatermark == false and settings.LR_reimportExportedPhoto == false)
    assert(settings.LR_export_colorSpace == 'sRGB' and settings.LR_jpeg_quality == 1)
end)
test('existing output is never overwritten', function()
    local p, root = fresh(); dirs[root] = true; blocked(p, root)
end)
test('offline/smart-preview-only original fails preflight', function()
    local p, root = fresh(); p.offline = true; blocked(p, root); assert(not dirs[root])
end)
test('HDR and video are explicit unsupported trial inputs', function()
    local p, root = fresh(); p.settings.HDREditMode = true; blocked(p, root)
    p.settings.HDREditMode = nil; p.metadata.isVideo = true; blocked(p, root)
end)
test('oversized input fails before output creation', function()
    local p, root = fresh(); oversized = true; blocked(p, root); assert(not dirs[root])
end)
test('editing during rendering leaves no complete manifest', function()
    local p, root = fresh(); hook = function(frame) frame.settings.CropLeft = 0.3 end; blocked(p, root)
end)
test('source mutation during rendering leaves no complete manifest', function()
    local p, root = fresh(); hook = function(frame) files[frame.metadata.path] = 'changed' end; blocked(p, root)
end)
test('cancellation leaves no complete manifest', function()
    local p, root = fresh(); hook = function() cancel = true end; blocked(p, root)
end)
test('earlier frame edits are rechecked before whole-batch publication', function()
    local a, root = fresh(); local b = photo(999, '/synthetic/second.JPG')
    hook = function(frame) if frame == b then a.metadata.lastEditTime = 101 end end
    assert(not pcall(Exporter.run, { a, b }, catalog, root, progress))
    assert(not files[root .. '/manifest.json'])
end)
test('virtual copies stay separate and duplicate LR identities fail', function()
    local a, root = fresh(); local b = photo(998, a.metadata.path); b.metadata.isVirtualCopy = true
    local result = Exporter.run({ a, b }, catalog, root, progress)
    assert(#result.images == 2 and result.images[2].is_virtual_copy)
    local c, other = fresh(); b.metadata.uuid = c.metadata.uuid
    assert(not pcall(Exporter.run, { c, b }, catalog, other, progress)); assert(not dirs[other])
end)
test('late crop-dimension changes are held even without an edit-counter increment', function()
    local a, root = fresh(); local b = photo(997, '/synthetic/late-crop.JPG')
    hook = function(frame) if frame == b then a.metadata.croppedDimensions.width = 39 end end
    assert(not pcall(Exporter.run, { a, b }, catalog, root, progress))
    assert(not files[root .. '/manifest.json'])
end)
test('settings order is canonical, cycles/sparse arrays/NaN are refused', function()
    assert(Codec.canonical({ a = 1, b = 2 }) == Codec.canonical({ b = 2, a = 1 }))
    local cycle = {}; cycle.self = cycle
    assert(not pcall(Codec.canonical, cycle))
    assert(not pcall(Codec.canonical, { [2] = 3 }))
    assert(not pcall(Codec.canonical, { a = 0 / 0 }))
end)
test('manifest move failure is not reported as a complete export', function()
    local p, root = fresh()
    local publish = Exporter.publish
    Exporter.publish = function(path, bytes) failMove = true; return publish(path, bytes) end
    blocked(p, root); Exporter.publish = publish; failMove = false
end)
test('trial size cap rejects empty/oversized selection before output', function()
    local p, root = fresh()
    assert(not pcall(Exporter.run, {}, catalog, root, progress))
    local many = {}; for i = 1, 11 do many[i] = p end
    assert(not pcall(Exporter.run, many, catalog, root, progress)); assert(not dirs[root])
end)

-- Cross-language fixture: exact Lua-produced JSON + actual JPEG bytes, all
-- synthetic and generated under ignored target/. Rust CI must validate it.
local base = repo .. '/target/lr-cross-language'
local p = photo(12345, base .. '/source.JPG')
local root = base .. '/export'
cancel = false; hook = nil; failMove = false; oversized = false
Exporter.run({ p }, catalog, root, progress)
for _, path in ipairs({ p.metadata.path, root .. '/render-0001.jpg', root .. '/manifest.json' }) do
    local f = assert(io.open(path, 'wb')); assert(f:write(files[path])); assert(f:close())
end
print(string.format('%d Lua orchestration tests passed; cross-language fixture generated', tests))

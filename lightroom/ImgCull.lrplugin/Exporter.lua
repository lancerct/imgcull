-- Adobe renders; Rust owns photo pairing, evidence, review state and metadata.
local File = import 'LrFileUtils'
local Path = import 'LrPathUtils'
local Digest = import 'LrDigest'
local Session = import 'LrExportSession'
local Date = import 'LrDate'
local App = import 'LrApplication'
local Codec = dofile(Path.child(_PLUGIN.path, 'Codec.lua'))
local M = { maxPhotos = 10 }
local maxBytes = 512 * 1024 * 1024

function M.newPath(parent, prefix, suffix)
    local name = prefix .. string.format('%.0f', Date.currentTime() * 1000) .. (suffix or '')
    return File.chooseUniqueFileName(Path.child(parent, name))
end

local function hashFile(path)
    assert(File.exists(path) == 'file', '原始文件或导出文件不可用：' .. tostring(path))
    local attributes = File.fileAttributes(path)
    assert(attributes.fileSize and attributes.fileSize > 0 and attributes.fileSize <= maxBytes,
        '文件为空或超过当前 512 MiB 限制')
    -- SDK readFile supports Unicode paths on Windows. Keep one source string
    -- at a time; this SDK provides no documented streaming file reader.
    return (Digest.SHA256.digest(File.readFile(path))):lower()
end

local function snapshot(photo)
    local settings = photo:getDevelopSettings()
    assert(type(settings) == 'table', '无法读取 Lightroom 当前后期设置')
    assert(not settings.HDREditMode or settings.HDREditMode == 0,
        '当前试运行不支持 HDR 成片，请先使用普通 SDR 照片验证')
    local dims = photo:getRawMetadata('croppedDimensions')
    local edited = photo:getRawMetadata('lastEditTime')
    local count = photo:getRawMetadata('editCount')
    assert(type(dims) == 'table' and type(dims.width) == 'number' and type(dims.height) == 'number',
        '无法读取成片尺寸')
    assert(dims.width > 0 and dims.height > 0, '成片尺寸无效')
    assert(type(edited) == 'number' and edited >= 0 and edited < math.huge and
        type(count) == 'number' and count >= 0 and count % 1 == 0, '无法确认 Lightroom 编辑状态')
    return {
        source = photo:getRawMetadata('path'),
        uuid = photo:getRawMetadata('uuid'),
        virtual = photo:getRawMetadata('isVirtualCopy') == true,
        develop_sha256 = (Digest.SHA256.digest(Codec.canonical(settings))):lower(),
        last_edit_time = edited,
        edit_count = count,
        cropped_width = math.floor(dims.width + 0.5),
        cropped_height = math.floor(dims.height + 0.5),
    }
end

-- Use an SDK move from a small temporary control file for Unicode destination
-- paths. Never fall back to shell execution or modify the selected photo.
function M.publish(path, bytes)
    assert(not File.exists(path), '拒绝覆盖已有导出记录')
    local temporary = M.newPath(Path.getStandardFilePath('temp'), 'imgcull-control-', '.json')
    local file, reason = io.open(temporary, 'wb')
    assert(file, '无法创建临时控制文件：' .. tostring(reason))
    local ok, writeReason = file:write(bytes)
    local closed, closeReason = file:close()
    if not ok or not closed then
        File.delete(temporary)
        error('写入导出记录失败：' .. tostring(writeReason or closeReason))
    end
    local moved, moveReason = File.move(temporary, path)
    if not moved then
        File.delete(temporary)
        error('发布导出记录失败：' .. tostring(moveReason))
    end
end

function M.run(photos, catalog, root, progress)
    assert(#photos >= 1 and #photos <= M.maxPhotos, '试运行请选择 1–10 张照片')
    assert(not File.exists(root), '导出目录已经存在，请选择新目录')
    -- Preflight every identity before creating output. Virtual copies stay
    -- distinct; this plugin never decides that shared files share a verdict.
    local before, ids = {}, {}
    for index, photo in ipairs(photos) do
        assert(not progress:isCanceled(), '已取消；未发布完整导出清单')
        assert(not photo:getRawMetadata('isVideo'), '试运行不处理视频')
        assert(photo:checkPhotoAvailability(), '原片不在线，不能只用智能预览')
        local state = snapshot(photo)
        assert(type(state.source) == 'string' and type(state.uuid) == 'string' and state.uuid ~= '', '无法读取照片身份')
        assert(not ids[state.uuid], '重复的 Lightroom 照片身份')
        ids[state.uuid] = true
        state.source_sha256 = hashFile(state.source)
        before[index] = state
    end
    File.createDirectory(root)
    assert(File.exists(root) == 'directory', '无法创建导出目录')
    local manifest = { schema = 1, exporter = 'imgcull-lr-v1',
        catalog = catalog:getPath(), lightroom_version = App.versionString(), images = {} }
    for index, photo in ipairs(photos) do
        assert(not progress:isCanceled(), '已取消；未发布完整导出清单')
        local staging = Path.child(root, string.format('.render-%04d', index))
        File.createDirectory(staging)
        assert(File.exists(staging) == 'directory', '无法创建渲染目录')
        local session = Session {
            photosToExport = { photo },
            exportSettings = {
                LR_exportServiceProvider = 'com.adobe.ag.export.file',
                LR_export_destinationType = 'specificFolder',
                LR_export_destinationPathPrefix = staging,
                LR_export_useSubfolder = false,
                LR_collisionHandling = 'skip',
                LR_renamingTokensOn = false,
                LR_format = 'JPEG', LR_jpeg_quality = 1,
                LR_jpeg_useLimitSize = false, LR_export_colorSpace = 'sRGB',
                LR_size_doConstrain = false, LR_outputSharpeningOn = false,
                LR_useWatermark = false, LR_reimportExportedPhoto = false,
                LR_export_postProcessing = 'doNothing',
                LR_minimizeEmbeddedMetadata = true,
                LR_embeddedMetadataOption = 'copyrightOnly',
                LR_removeLocationMetadata = true, LR_removeFaceMetadata = true,
            },
        }
        local result, renditionCount = nil, 0
        for _, rendition in session:renditions({ progressScope = progress, stopIfCanceled = true }) do
            renditionCount = renditionCount + 1
            assert(renditionCount == 1, '单张导出返回了多个结果')
            local success, rendered = rendition:waitForRender()
            assert(success, 'Lightroom 渲染失败：' .. tostring(rendered))
            assert(Path.standardizePath(Path.parent(rendered)) == Path.standardizePath(staging), '渲染输出不在指定目录')
            result = rendered
        end
        assert(result and not progress:isCanceled(), '已取消或未生成完整成片')
        local after = snapshot(photo)
        assert(Codec.canonical(after) == Codec.canonical({
            source = before[index].source, uuid = before[index].uuid, virtual = before[index].virtual,
            develop_sha256 = before[index].develop_sha256, last_edit_time = before[index].last_edit_time,
            edit_count = before[index].edit_count, cropped_width = before[index].cropped_width,
            cropped_height = before[index].cropped_height,
        }), '导出期间照片或后期发生变化，请停止编辑后重试')
        assert(hashFile(after.source) == before[index].source_sha256, '导出期间原片内容发生变化')
        local name = string.format('render-%04d.jpg', index)
        local target = Path.child(root, name)
        assert(not File.exists(target), '拒绝覆盖已有成片')
        assert(File.move(result, target), '无法发布成片')
        manifest.images[index] = {
            photo_uuid = after.uuid, local_identifier = tostring(photo.localIdentifier),
            is_virtual_copy = after.virtual, source = after.source,
            source_sha256 = before[index].source_sha256,
            develop_sha256 = after.develop_sha256, last_edit_time = after.last_edit_time,
            edit_count = after.edit_count, cropped_width = after.cropped_width,
            cropped_height = after.cropped_height, file = name, sha256 = hashFile(target),
        }
        progress:setPortionComplete(index, #photos)
    end
    -- Recheck all reported LR states and originals immediately before publishing
    -- the batch, including frames rendered earlier. This is not a catalog lock.
    for index, photo in ipairs(photos) do
        local now = snapshot(photo)
        assert(now.develop_sha256 == before[index].develop_sha256 and
            now.last_edit_time == before[index].last_edit_time and now.edit_count == before[index].edit_count and
            now.source == before[index].source and now.uuid == before[index].uuid and now.virtual == before[index].virtual and
            now.cropped_width == before[index].cropped_width and now.cropped_height == before[index].cropped_height,
            '导出完成前检测到编辑变化；没有发布完整清单')
        assert(hashFile(now.source) == before[index].source_sha256, '导出完成前原片发生变化')
    end
    assert(not progress:isCanceled(), '已取消；未发布完整导出清单')
    M.publish(Path.child(root, 'manifest.json'), Codec.json.encode(manifest))
    return manifest
end

return M

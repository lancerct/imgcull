local Tasks = import 'LrTasks'
local Dialogs = import 'LrDialogs'
local App = import 'LrApplication'
local Path = import 'LrPathUtils'
local Progress = import 'LrProgressScope'
local prefs = import 'LrPrefs'.prefsForPlugin()
if prefs.busy then
    Dialogs.showBezel('ImgCull 正在导出；可在 LR 的进度区取消', 3)
    return
end
prefs.busy = true
Tasks.startAsyncTask(function()
    local progress
    local ok, result = Tasks.pcall(function()
        local catalog = App.activeCatalog()
        assert(catalog:getTargetPhoto(), '请先在图库中明确选中 1–10 张照片')
        local photos = catalog:getTargetPhotos()
        local Exporter = dofile(Path.child(_PLUGIN.path, 'Exporter.lua'))
        assert(#photos >= 1 and #photos <= Exporter.maxPhotos, '试运行请选择 1–10 张照片')
        local destinations = Dialogs.runOpenPanel {
            title = '选择试导出存放目录（不写入原片）',
            canChooseFiles = false, canChooseDirectories = true, allowsMultipleSelection = false,
        }
        if not destinations or #destinations == 0 then return nil end
        local root = Exporter.newPath(destinations[1], '.imgcull-lr-')
        prefs.lastOutput = root
        prefs.lastError = nil
        progress = Progress { title = 'ImgCull：后台导出当前后期效果' }
        Exporter.run(photos, catalog, root, progress)
        return root
    end)
    if progress then progress:done() end
    prefs.busy = false
    if not ok then
        prefs.lastError = tostring(result)
        Dialogs.showBezel('ImgCull 导出未完成；可从插件菜单查看原因', 4)
    elseif result then
        prefs.lastError = nil
        Dialogs.showBezel('ImgCull 试导出完成；可从插件菜单查看目录', 4)
    end
end)

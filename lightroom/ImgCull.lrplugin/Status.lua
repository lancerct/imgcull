local Dialogs = import 'LrDialogs'
local prefs = import 'LrPrefs'.prefsForPlugin()
local state = prefs.busy and '正在后台导出。' or '当前没有导出任务。'
local details = state .. '\n\n输出目录：\n' .. tostring(prefs.lastOutput or '尚未导出')
if prefs.lastError then details = details .. '\n\n最近一次未完成的原因：\n' .. prefs.lastError end
Dialogs.message('ImgCull 最近导出状态', details, 'info')

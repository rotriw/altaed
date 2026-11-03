#!/usr/bin/env python3
"""Offline conversion of pinned upstream VS Code palettes into Zed + web data.

Source JSON/JSONC and licenses live beside the generated family. No package
scripts or network requests run here; SOURCE.json records upstream revisions.
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'crates/altaed/assets/themes'
SOURCES = ASSETS / 'sources'


def read_jsonc(path):
    text = path.read_text()
    # Keep quoted URLs, escapes and comment-like source text intact.
    cleaned = re.sub(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/', lambda match: match[0] if match[0].startswith('"') else '', text)
    cleaned = re.sub(r'("(?:\\.|[^"\\])*")|,\s*(?=[}\]])', lambda match: match[1] or '', cleaned)
    value = json.loads(cleaned)
    if value.get('include'):
        base = read_jsonc(path.parent / Path(value['include']).name)
        value['colors'] = {**base.get('colors', {}), **value.get('colors', {})}
        value['tokenColors'] = base.get('tokenColors', []) + value.get('tokenColors', [])
    return value


def mix(a, b, weight):
    x, y = [int(a[1:7], 16), int(b[1:7], 16)]
    return '#' + ''.join(f'{round(((x >> shift) & 255)*(1-weight)+((y >> shift) & 255)*weight):02x}' for shift in [16, 8, 0])


def syntax_color_on_surface(color, background):
    # GPUI TextStyle.highlight blends alpha against the foreground, whereas
    # VS Code renders it over the editor surface. Resolve that difference once
    # during conversion so code and lightweight previews agree on both engines.
    if len(color) != 9:
        return color
    alpha = int(color[7:9], 16) / 255
    return mix(background, color, alpha)


def contrast(a, b):
    def luminance(color):
        channels = [int(color[index:index + 2], 16) / 255 for index in [1, 3, 5]]
        linear = [value / 12.92 if value <= .04045 else ((value + .055) / 1.055) ** 2.4 for value in channels]
        return sum(value * weight for value, weight in zip(linear, [.2126, .7152, .0722]))
    x, y = luminance(a), luminance(b)
    return (max(x, y) + .05) / (min(x, y) + .05)


def readable_ui_color(color, surfaces):
    # Adapt UI text independently of the pinned syntax palette. Check selected
    # and elevated surfaces too; a color readable on the editor may fail in menus.
    target = max(['#000000', '#ffffff'], key=lambda ink: min(contrast(ink, bg) for bg in surfaces))
    # Reserve a small margin for 8-bit surface rounding in native rendering.
    for step in range(101):
        candidate = mix(color, target, step / 100)
        if min(contrast(candidate, bg) for bg in surfaces) >= 4.55:
            return candidate
    raise ValueError('No readable UI foreground for theme surfaces')


CAPTURES = {
    'attribute':'entity.other.attribute-name', 'boolean':'constant.language.boolean',
    'comment':'comment', 'constant':'constant', 'constant.builtin':'constant.language',
    'constructor':'entity.name.type', 'enum':'entity.name.type', 'function':'entity.name.function',
    'function.builtin':'support.function', 'function.call':'entity.name.function', 'function.special':'support.function',
    'keyword':'keyword', 'keyword.control':'keyword.control', 'label':'entity.name.label',
    'module':'entity.name.type', 'namespace':'entity.name.type', 'concept':'entity.name.type',
    'number':'constant.numeric', 'operator':'keyword.operator', 'operator.spaceship':'keyword.operator',
    'preproc':'keyword.control', 'property':'support.type.property-name',
    'punctuation':'punctuation', 'punctuation.bracket':'punctuation.section', 'punctuation.delimiter':'punctuation.separator',
    'string':'string.quoted', 'string.escape':'constant.character.escape', 'string.regex':'string.regexp',
    'type':'entity.name.type', 'type.builtin':'support.type', 'variable':'variable', 'variable.builtin':'variable.language',
    'variable.special':'variable.language', 'tag':'entity.name.tag', 'text.literal':'markup.inline.raw',
    'title':'markup.heading', 'emphasis':'markup.italic', 'emphasis.strong':'markup.bold', 'link_uri':'markup.underline.link',
}


def syntax_style(source, target, fallback, background):
    matches = []
    for index, rule in enumerate(source.get('tokenColors', [])):
        scopes = rule.get('scope', [])
        if isinstance(scopes, str): scopes = scopes.split(',')
        for scope in scopes:
            scope = scope.strip()
            if not scope or ' ' in scope: continue
            if target == scope or target.startswith(scope + '.'):
                matches.append((len(scope), index, rule.get('settings', {})))
    result = {'color': fallback}
    for _, _, settings in sorted(matches):
        if settings.get('foreground'): result['color'] = settings['foreground']
        if 'fontStyle' in settings:
            words = settings['fontStyle'].split()
            result.update(font_style='italic' if 'italic' in words else 'normal', font_weight=700 if 'bold' in words else 400)
    result['color'] = syntax_color_on_surface(result['color'], background)
    return result


def convert(identifier, label, filename, appearance):
    source = read_jsonc(SOURCES / filename)
    c = source.get('colors', {})
    light = appearance == 'light'
    background = c.get('editor.background', '#ffffff' if light else '#1e1e1e')
    editor_text = c.get('editor.foreground', c.get('foreground', '#333333' if light else '#d4d4d4'))
    text = c.get('foreground', editor_text)
    # Thin separators must remain visible on dark editor surfaces; upstream
    # translucent dividers can disappear when ported out of VS Code's layout.
    border = c.get('editorGroup.border', mix(background, text, .12)) if light else mix(background, text, .16)
    subtle = mix(background, text, .025 if light else .04)
    accent = c.get('textLink.foreground', c.get('activityBarBadge.background', '#315e91' if light else '#74ade8'))
    selected = c.get('editor.selectionBackground', mix(background, accent, .18))
    elevated = c.get('editorWidget.background', subtle)
    surfaces = [background, subtle, elevated,
                syntax_color_on_surface(selected, background),
                syntax_color_on_surface(selected, subtle),
                syntax_color_on_surface(selected, elevated)]
    ui_text = readable_ui_color(text, surfaces)
    muted = readable_ui_color(mix(text, background, .38), surfaces)
    text_accent = readable_ui_color(accent, surfaces)
    error = c.get('editorError.foreground', '#bc334a' if light else '#f48771')
    success = '#247347' if light else '#70c49b'
    style = {
        'background':background, 'surface.background':subtle, 'elevated_surface.background':elevated,
        'border':border, 'border.variant':border, 'border.focused':accent, 'border.selected':accent,
        'border.transparent':'#00000000', 'border.disabled':border,
        'element.background':background, 'element.hover':subtle, 'element.active':subtle, 'element.selected':selected,
        'element.disabled':subtle, 'ghost_element.background':'#00000000', 'ghost_element.hover':'#00000000',
        'ghost_element.active':'#00000000', 'ghost_element.selected':selected, 'ghost_element.disabled':'#00000000',
        'text':ui_text, 'text.muted':muted, 'text.placeholder':muted, 'text.disabled':muted, 'text.accent':text_accent,
        'icon':ui_text, 'icon.muted':muted, 'icon.accent':accent,
        'status_bar.background':background, 'title_bar.background':background, 'title_bar.inactive_background':subtle,
        'toolbar.background':background, 'tab_bar.background':subtle, 'tab.active_background':background,
        'tab.inactive_background':subtle, 'panel.background':background,
        'editor.background':background, 'editor.foreground':editor_text, 'editor.gutter.background':background,
        'editor.line_number':mix(editor_text, background, .52), 'editor.active_line_number':muted,
        'editor.active_line.background':c.get('editor.lineHighlightBackground', subtle),
        'editor.highlighted_line.background':subtle, 'editor.subheader.background':subtle,
        'editor.indent_guide':mix(background, text, .12), 'editor.indent_guide_active':mix(background, text, .24),
        'editor.invisible':mix(editor_text, background, .55),
        'search.match_background':c.get('editor.findMatchBackground', selected),
        'scrollbar.thumb.background':mix(background, text, .15), 'scrollbar.thumb.hover_background':mix(background, text, .25),
        'scrollbar.track.background':'#00000000',
        'terminal.background':background, 'terminal.foreground':editor_text,
        'error':readable_ui_color(error, surfaces), 'error.background':mix(background, error, .09), 'error.border':mix(background,error,.25),
        'success':readable_ui_color(success, surfaces), 'success.background':mix(background,success,.08), 'success.border':mix(background,success,.25),
        'warning':readable_ui_color(c.get('editorWarning.foreground', '#936b1e' if light else '#e5c07b'), surfaces),
        'players':[{'cursor':c.get('editorCursor.foreground',accent),'background':selected,'selection':selected}],
        'syntax':{capture:syntax_style(source, scope, editor_text, background) for capture, scope in CAPTURES.items()},
    }
    for key,value in c.items():
        if key.startswith('terminal.ansi'):
            name=re.sub(r'([A-Z])',lambda match:'_'+match[1].lower(),key[len('terminal.ansi'):]).lstrip('_')
            style['terminal.ansi.'+name.replace('bright_','bright_')]=value
    if identifier == 'default-light':
        style.update({'background':'#ffffff','surface.background':'#f6f7f9','panel.background':'#f9fafb','text':'#27313b','text.muted':'#5b6672','text.placeholder':'#5b6672','text.accent':'#315e91','border':'#dfe3e8','element.selected':'#e7eef7','ghost_element.selected':'#e7eef7','error':'#a83232'})
    return {'id':identifier,'label':label,'name':label,'appearance':appearance,'style':style}


def main():
    definitions = [
        ('default-dark','默认深色 (Dark)','microsoft__vscode/dark_plus.json','dark'),
        ('default-light','默认浅色 (Light)','microsoft__vscode/light_plus.json','light'),
        ('monokai','Monokai','microsoft__vscode/monokai-color-theme.json','dark'),
        ('github-light','GitHub Light','primer__github-vscode-theme/light-default.json','light'),
        ('github-dark','GitHub Dark','primer__github-vscode-theme/dark-default.json','dark'),
        ('solarized-light','Solarized Light','microsoft__vscode/solarized-light-color-theme.json','light'),
        ('solarized-dark','Solarized Dark','microsoft__vscode/solarized-dark-color-theme.json','dark'),
        ('dracula','Dracula','dracula__visual-studio-code/dracula.json','dark'),
    ]
    horizon = read_jsonc(SOURCES/'jolaleye__horizon-theme-vscode/package.json')
    for theme in horizon['contributes']['themes']:
        filename=Path(theme['path']).name
        definitions.append((filename.removesuffix('.json'),theme['label'],'jolaleye__horizon-theme-vscode/'+filename,'light' if theme['uiTheme']=='vs' else 'dark'))
    family={'name':'altaed bundled themes','author':'Upstream theme authors; altaed adaptation','themes':[convert(*item) for item in definitions]}
    data=json.dumps(family,ensure_ascii=False,indent=2)+'\n'
    (ASSETS/'bundled.json').write_text(data)
    (ROOT/'prototypes/test-panel/themes.json').write_text(data)
    print(f'Converted {len(definitions)} pinned themes offline.')

if __name__ == '__main__': main()

"Local reference names for included document fragments."
from fast5ever import Element


def _walk(node):
    for child in node.children:
        if isinstance(child, Element):
            yield child
            yield from _walk(child)


def scope_ids(root):
    "Prefix include-local ids and links with scope:."
    includes = [e for e in _walk(root) if 'include' in e.attrs.get('class', '').split() and 'scope' in e.attrs]
    scopes = [e.attrs['scope'] for e in includes]
    if len(scopes) != len(set(scopes)): raise ValueError('include scopes must be unique')
    for inc in reversed(includes):
        scope = inc.attrs.pop('scope')
        if not scope: raise ValueError('include scope must not be empty')
        els = list(_walk(inc))
        ids = {e.attrs['id']: f"{scope}:{e.attrs['id']}" for e in els if 'id' in e.attrs}
        for e in els:
            if (id := e.attrs.get('id')) in ids: e.attrs['id'] = ids[id]
            href = e.attrs.get('href', '')
            if href.startswith('#') and href[1:] in ids: e.attrs['href'] = '#' + ids[href[1:]]
    return root

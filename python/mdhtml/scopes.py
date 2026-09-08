"Local reference names for included document fragments."
from fast5ever import Element


def _walk(node):
    for child in node.children:
        if isinstance(child, Element):
            yield child
            yield from _walk(child)


def scope_ids(root):
    "Namespace include-local ids and links in place, preserving the reference type before the first hyphen."
    includes = [e for e in _walk(root) if 'include' in e.attrs.get('class', '').split() and 'scope' in e.attrs]
    scopes = [e.attrs['scope'] for e in includes]
    if len(scopes) != len(set(scopes)): raise ValueError('include scopes must be unique')
    for inc in reversed(includes):
        scope = inc.attrs.pop('scope')
        if not scope: raise ValueError('include scope must not be empty')
        els = list(_walk(inc))
        def scoped(id):
            typ, sep, rest = id.partition('-')
            return f'{typ}-{scope}{rest}' if sep else scope + id
        ids = {e.attrs['id']: scoped(e.attrs['id']) for e in els if 'id' in e.attrs}
        for e in els:
            if (id := e.attrs.get('id')) in ids: e.attrs['id'] = ids[id]
            href = e.attrs.get('href', '')
            if href.startswith('#') and href[1:] in ids: e.attrs['href'] = '#' + ids[href[1:]]
    return root

// A component's instance script must not reach a top-level binding before its
// declaration by what runs during init (da-1oi8w). `vite dev` keeps the `const`
// and throws a TDZ error, a 500; a production build inlines it, so the fault
// shows nowhere else, and TS does not follow closures (#1445's HANDLE_PX).
//
// What runs during init: each top-level statement in order, the arguments of
// its calls, a callback handed to any call but those in DEFERRED, and the local
// functions those call or hand on. A `$derived` runs when it is first read, so
// its initializer counts where a read of it does. Every branch counts as taken
// but one a `let` still holding its falsy start decides: `if (!el) return`, for
// an `el` the template binds only once init is over. A block's rest after a
// plain `await` runs later. What it does not see: a function reached through an
// object, an alias or an import.

import { parse } from 'svelte/compiler';

/** Calls whose callback does not run during init, by the name they are called with. */
const DEFERRED = new Set([
	'$effect',
	'onMount',
	'onDestroy',
	'beforeNavigate',
	'afterNavigate',
	'onNavigate',
	'setTimeout',
	'setInterval',
	'requestAnimationFrame',
	'requestIdleCallback',
	'whenIdle',
	'queueMicrotask',
	'addEventListener',
	'removeEventListener',
	'then',
	'catch',
	'finally',
	'set',
	'push',
	'ResizeObserver',
	'IntersectionObserver',
	'MutationObserver'
]);

const isFunction = (n) => /^(FunctionDeclaration|FunctionExpression|ArrowFunctionExpression)$/.test(n?.type);

/** The name a call is made by: `$effect.pre`, or `el.addEventListener`'s `addEventListener`. */
function calleeName(c) {
	if (c.type === 'Identifier') return c.name;
	if (c.type !== 'MemberExpression' || c.computed) return '';
	return c.object.name?.startsWith('$') ? `${c.object.name}.${c.property.name}` : c.property.name;
}

const isCallOf = (n, pattern) => n?.type === 'CallExpression' && pattern.test(calleeName(n.callee));

/** An initializer that leaves a `let` falsy: none, `$state()`, a falsy literal. */
function startsFalsy(init) {
	if (isCallOf(init, /^\$state(\.raw)?$/)) return !init.arguments[0] || startsFalsy(init.arguments[0]);
	if (!init) return true;
	return init.type === 'Literal' ? !init.value : init.type === 'Identifier' && init.name === 'undefined';
}

/** The names a pattern binds. */
function names(p, out = []) {
	if (p?.type === 'Identifier') out.push(p.name);
	else if (p?.type === 'ObjectPattern') p.properties.forEach((q) => names(q.value ?? q.argument, out));
	else if (p?.type === 'ArrayPattern') p.elements.forEach((q) => names(q, out));
	else if (p?.type === 'AssignmentPattern') names(p.left, out);
	else if (p?.type === 'RestElement') names(p.argument, out);
	return out;
}

/** What a block's own statements declare, which shadows the top level: name → its function, if it is one. */
function declared(statements) {
	const scope = new Map();
	for (const s of statements) {
		if (s.type === 'FunctionDeclaration') scope.set(s.id.name, s);
		else if (s.type === 'ClassDeclaration') scope.set(s.id.name, null);
		else if (s.type === 'VariableDeclaration') {
			for (const d of s.declarations) {
				for (const name of names(d.id)) scope.set(name, d.id.type === 'Identifier' && isFunction(d.init) ? d.init : null);
			}
		}
	}
	return scope;
}

/** A statement that always gives way to the microtask queue before the next one. */
const suspends = (s) =>
	(s.type === 'ExpressionStatement' && s.expression.type === 'AwaitExpression') ||
	(s.type === 'VariableDeclaration' && s.declarations.some((d) => d.init?.type === 'AwaitExpression'));

/** A branch that does not fall through to the statement after it. */
const leaves = (s) => /^(Return|Throw)Statement$/.test(s.type) || (s.type === 'BlockStatement' && s.body.some(leaves));

/** Each read before its declaration: `{name, line, declaredAt, path}`, `path` how init got there. */
export function findings(source, filename = 'component.svelte') {
	const program = parse(source, { modern: true, filename }).instance?.content;
	if (!program) return [];
	const lineOf = (n) => source.slice(0, n.start).split('\n').length;
	const statements = program.body.map((s) => (s.type === 'ExportNamedDeclaration' && s.declaration ? s.declaration : s));

	const top = declared(statements);
	const lexical = new Map();
	const deriveds = new Map();
	const falsy = new Set();
	for (const s of statements) {
		if (s.type === 'ClassDeclaration') lexical.set(s.id.name, s);
		if (s.type !== 'VariableDeclaration') continue;
		for (const d of s.declarations) {
			names(d.id).forEach((name) => lexical.set(name, d));
			if (d.id.type !== 'Identifier') continue;
			if (isCallOf(d.init, /^\$derived(\.by)?$/)) deriveds.set(d.id.name, d.init);
			if (s.kind === 'let' && startsFalsy(d.init)) falsy.add(d.id.name);
		}
	}

	const live = new Set();
	const found = new Map();
	const seen = new Set();
	let step = 0;

	const shadowed = (name, shadow) => shadow.some((scope) => scope.has(name));

	/** The function `name` holds where it is read, and the scopes around it. */
	function functionOf(name, shadow) {
		for (let i = shadow.length - 1; i >= 0; i--) {
			if (shadow[i].has(name)) return { fn: shadow[i].get(name), around: shadow.slice(0, i + 1) };
		}
		return { fn: top.get(name), around: [] };
	}

	function read(id, shadow, path) {
		if (shadowed(id.name, shadow)) return;
		if (lexical.has(id.name) && !live.has(id.name)) {
			const key = `${id.name}:${id.start}`;
			if (!found.has(key)) found.set(key, { name: id.name, line: lineOf(id), declaredAt: lineOf(lexical.get(id.name)), path });
		}
		const derived = deriveds.get(id.name);
		if (derived && !seen.has(`${derived.start}:${step}`)) {
			seen.add(`${derived.start}:${step}`);
			const via = [...path, `${id.name} (line ${lineOf(derived)})`];
			if (isCallOf(derived, /\.by$/)) follow(derived.arguments[0], [], via, 'its function');
			else walk(derived.arguments[0], [], via);
		}
	}

	/** Whether `e` is falsy while its `let`s hold their start. */
	function isFalsy(e, shadow) {
		if (e.type === 'Identifier') return falsy.has(e.name) && !shadowed(e.name, shadow);
		if (e.type === 'ChainExpression') {
			// `el?.a.b()`: undefined while `el` is, whatever follows it.
			let root = e.expression;
			while (root.object ?? root.callee) root = root.object ?? root.callee;
			return isFalsy(root, shadow);
		}
		if (e.type === 'LogicalExpression' && e.operator === '&&') return isFalsy(e.left, shadow) || isFalsy(e.right, shadow);
		if (e.type === 'LogicalExpression' && e.operator === '||') return isFalsy(e.left, shadow) && isFalsy(e.right, shadow);
		return e.type === 'UnaryExpression' && e.operator === '!' && isTruthy(e.argument, shadow);
	}
	function isTruthy(e, shadow) {
		if (e.type === 'LogicalExpression' && e.operator === '||') return isTruthy(e.left, shadow) || isTruthy(e.right, shadow);
		if (e.type === 'LogicalExpression' && e.operator === '&&') return isTruthy(e.left, shadow) && isTruthy(e.right, shadow);
		return e.type === 'UnaryExpression' && e.operator === '!' && isFalsy(e.argument, shadow);
	}

	/** Run a function's body, once for each point of init that reaches it. */
	function enter(fn, around, path) {
		const key = `${fn.start}:${step}`;
		if (seen.has(key)) return;
		seen.add(key);
		const scope = new Map(fn.params.flatMap((p) => names(p)).map((name) => [name, null]));
		const inner = [...around, scope];
		walk(fn.params, inner, path, true);
		walk(fn.body, inner, path);
	}

	/** Call `n`: an inline function, or a local one named. */
	function follow(n, shadow, path, label) {
		if (isFunction(n)) return enter(n, shadow, [...path, label]);
		read(n, shadow, path);
		const { fn, around } = functionOf(n.name, shadow);
		if (fn) enter(fn, around, [...path, `${n.name} (line ${lineOf(fn)})`]);
	}

	function walk(n, shadow, path, pattern = false) {
		if (Array.isArray(n)) return n.forEach((x) => walk(x, shadow, path, pattern));
		if (!n || typeof n.type !== 'string') return;
		if (isFunction(n) || n.type === 'ClassDeclaration' || n.type === 'ClassExpression') return;
		const on = (x, as = pattern) => walk(x, shadow, path, as);
		switch (n.type) {
			case 'Identifier':
				return void (pattern || read(n, shadow, path));
			case 'AssignmentPattern':
				on(n.left);
				return on(n.right, false);
			case 'MemberExpression':
				on(n.object, false);
				return void (n.computed && on(n.property, false));
			case 'Property':
				if (n.computed) on(n.key, false);
				return on(n.value);
			case 'CallExpression':
			case 'NewExpression': {
				const name = calleeName(n.callee);
				const local = (x) => x.type === 'Identifier' && functionOf(x.name, shadow).fn;
				if (isFunction(n.callee) || local(n.callee)) follow(n.callee, shadow, path, 'an immediate call');
				else on(n.callee, false);
				for (const a of n.arguments) {
					if (!isFunction(a) && !local(a)) on(a, false);
					else if (DEFERRED.has(name)) a.type === 'Identifier' && read(a, shadow, path);
					else follow(a, shadow, path, `${name || 'a call'}'s callback`);
				}
				return;
			}
			case 'BlockStatement': {
				const inner = [...shadow, declared(n.body)];
				for (const s of n.body) {
					walk(s, inner, path);
					if ((s.type === 'IfStatement' && isTruthy(s.test, inner) && leaves(s.consequent)) || suspends(s)) return;
				}
				return;
			}
			case 'IfStatement':
			case 'ConditionalExpression':
				on(n.test, false);
				if (!isFalsy(n.test, shadow)) on(n.consequent, false);
				if (!isTruthy(n.test, shadow)) on(n.alternate, false);
				return;
			case 'LogicalExpression':
				on(n.left, false);
				if (n.operator === '&&' ? !isFalsy(n.left, shadow) : n.operator !== '||' || !isTruthy(n.left, shadow)) on(n.right, false);
				return;
			case 'AssignmentExpression':
			case 'UpdateExpression': {
				const target = n.left ?? n.argument;
				on(n.right, false);
				on(target, false);
				if (target.type === 'Identifier' && !shadowed(target.name, shadow)) falsy.delete(target.name);
				return;
			}
			case 'VariableDeclarator':
				on(n.id, true);
				return on(n.init, false);
			case 'ForStatement':
			case 'ForInStatement':
			case 'ForOfStatement': {
				const head = n.init ?? n.left;
				const inner = [...shadow, head?.type === 'VariableDeclaration' ? declared([head]) : new Map()];
				for (const k of ['init', 'left', 'right', 'test', 'update', 'body']) walk(n[k], inner, path);
				return;
			}
			case 'CatchClause':
				on(n.body, false);
				return walk(n.body, [...shadow, new Map(names(n.param).map((name) => [name, null]))], path);
			case 'SwitchStatement':
				on(n.discriminant, false);
				return walk(n.cases, [...shadow, declared(n.cases.flatMap((c) => c.consequent))], path);
			case 'LabeledStatement':
				return on(n.body, false);
			case 'BreakStatement':
			case 'ContinueStatement':
				return;
		}
		// A type names nothing that runs; `x as T` and `x!` keep their `expression`.
		if (n.type.startsWith('TS')) return n.expression && on(n.expression, false);
		for (const key of Object.keys(n)) {
			if (/^(typeAnnotation|returnType|typeParameters|typeArguments)$/.test(key)) continue;
			if (n[key] && typeof n[key] === 'object') on(n[key]);
		}
	}

	for (const s of statements) {
		if (s.type === 'VariableDeclaration') {
			for (const d of s.declarations) {
				step++;
				if (d.id.type !== 'Identifier' || !deriveds.has(d.id.name)) walk(d, [], [`line ${lineOf(d)}`]);
				names(d.id).forEach((name) => live.add(name));
			}
		} else if (s.type === 'ClassDeclaration') {
			live.add(s.id.name);
		} else if (s.type !== 'ImportDeclaration' && s.type !== 'ExportAllDeclaration' && !s.type.startsWith('TS')) {
			step++;
			walk(s, [], [`line ${lineOf(s)}`]);
		}
	}
	return [...found.values()];
}

export const say = (file, f) =>
	`${file}:${f.line}: ${f.name} is read before its declaration on line ${f.declaredAt}, during init: ${f.path.join(' → ')}`;

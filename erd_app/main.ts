import init, { Globe } from '../erd_viewer/pkg/erd_viewer';
import { buildDate } from './build-date';

const ZOOM_SPEED = 0.02;
const ROT_SPEED = 0.005;
const MAX_DIST = 5;
const MIN_DIST = 1.2;
const TWIST_BASE = 0.001; // radians per render frame
const TWIST_ACCEL = 0.001;
const TWIST_MAX = 0.06;

type ControlKey = 'q' | 'w' | 'e' | 'a' | 's' | 'd';
const holdingKey: Record<ControlKey, number> = { q: 0, w: 0, e: 0, a: 0, s: 0, d: 0 };
const holdingKeyAccel = (key: ControlKey) => { holdingKey[key] = Math.min(TWIST_MAX, holdingKey[key] + TWIST_ACCEL); };

bootup();

async function bootup() {
	console.log(`Built on ${buildDate}`);
	await init();
	const canvas = document.getElementById('globe') as HTMLCanvasElement;
	canvas.width = canvas.clientWidth * window.devicePixelRatio;
	canvas.height = canvas.clientHeight * window.devicePixelRatio;

	console.log('before');
	const globe = new Globe(canvas);
	console.log('after');
	setupControls(canvas, globe);
	enableTouchGestures(canvas, globe);

	window.addEventListener('resize', () => resizeCanvasToDisplaySize(canvas));
	resizeCanvasToDisplaySize(canvas);

	// globe.set_image(await loadImage('./age.2020.1.GTS2012.webp'));
	renderOnAnimationFrame(globe);
}

function renderOnAnimationFrame(globe: Globe, onFrame?: () => void) {
	function frame() {
		onFrame?.();
		if (holdingKey.q) {
			globe.apply_twist(-holdingKey.q);
			holdingKeyAccel('q');
		}
		if (holdingKey.e) {
			globe.apply_twist(holdingKey.e);
			holdingKeyAccel('e');
		}
		if (holdingKey.w) {
			globe.apply_drag(0, 1, holdingKey.w);
			holdingKeyAccel('w');
		}
		if (holdingKey.s) {
			globe.apply_drag(0, -1, holdingKey.s);
			holdingKeyAccel('s');
		}
		if (holdingKey.a) {
			globe.apply_drag(1, 0, holdingKey.a);
			holdingKeyAccel('a');
		}
		if (holdingKey.d) {
			globe.apply_drag(-1, 0, holdingKey.d);
			holdingKeyAccel('d');
		}
		globe.render();
		requestAnimationFrame(frame);
	}
	requestAnimationFrame(frame);
}

function resizeCanvasToDisplaySize(canvas: HTMLCanvasElement) {
	const dpr = window.devicePixelRatio || 1;
	const displayWidth = Math.floor(canvas.clientWidth * dpr);
	const displayHeight = Math.floor(canvas.clientHeight * dpr);

	if (canvas.width !== displayWidth || canvas.height !== displayHeight) {
		canvas.width = displayWidth;
		canvas.height = displayHeight;
	}
}

function setupControls(canvas: HTMLCanvasElement, globe: Globe) {
	if (isTouchCapable()) { return; }

	let dragging = false;
	let lastX = 0, lastY = 0;
	let dist = 2.2;

	canvas.addEventListener('wheel', e => {
		e.preventDefault();
		const k = 1.0 - Math.sign(e.deltaY) * ZOOM_SPEED; // zoom step
		dist = Math.min(MAX_DIST, Math.max(MIN_DIST, dist * k));
		globe.set_distance(dist);
	}, { passive: false });

	window.addEventListener('keydown', e => {
		if (e.target !== document.body // avoid typing fields
			|| !/^[qewsad]$/i.test(e.key)
		) {
			return;
		}
		const key = e.key.toLocaleLowerCase() as ControlKey;
		if (!holdingKey[key]) {
			holdingKey[key] = TWIST_BASE;
		}
	});
	window.addEventListener('keyup', e => {
		if (e.target !== document.body // avoid typing fields
			|| !/^[qewsad]$/i.test(e.key)
		) {
			return;
		}
		const key = e.key.toLocaleLowerCase() as ControlKey;
		holdingKey[key] = 0;
	});

	canvas.addEventListener('pointerdown', e => {
		dragging = true;
		lastX = e.clientX;
		lastY = e.clientY;
		canvas.setPointerCapture(e.pointerId);
	});

	canvas.addEventListener('pointermove', e => {
		if (!dragging) return;
		const dx = e.clientX - lastX;
		const dy = e.clientY - lastY;
		lastX = e.clientX;
		lastY = e.clientY;

		const rotSpeed = ROT_SPEED * dist / 5;
		globe.apply_drag(dx, dy, rotSpeed);
	});

	canvas.addEventListener('pointerup', e => {
		dragging = false;
		canvas.releasePointerCapture(e.pointerId);
	});
}

function isTouchCapable() {
	// Prefer Pointer Events + maxTouchPoints, fallback to older signals
	return (window.PointerEvent && navigator.maxTouchPoints > 0)
		|| (window.matchMedia?.('(hover: none) and (pointer: coarse)').matches)
		|| ('ontouchstart' in window);
}

function enableTouchGestures(canvas: HTMLCanvasElement, globe: Globe, { minDist = MIN_DIST, maxDist = MAX_DIST } = {}) {
	if (!isTouchCapable()) { return; } // do nothing on non-touch

	const touches = new Map<number, PointerEvent>();
	let dragging = false;
	let lastX = 0, lastY = 0;
	let twistLast: number | null = null;
	let pinchStart = 0, pinchDist = 0;
	let dist = 2.2; // keep your current dist source if stored elsewhere

	function onDown(e: PointerEvent) {
		touches.set(e.pointerId, e);
		if (touches.size === 1) {
			dragging = true;
			lastX = e.clientX;
			lastY = e.clientY;
		} else if (touches.size === 2) {
			const [p1, p2] = [...touches.values()];
			twistLast = vecAngle(p1, p2);
			pinchStart = pinchLen(p1, p2);
			pinchDist = dist;
		}
		canvas.setPointerCapture(e.pointerId);
	}

	function onMove(e: PointerEvent) {
		touches.set(e.pointerId, e);
		if (touches.size === 1 && dragging) {
			const dx = e.clientX - lastX;
			const dy = e.clientY - lastY;
			lastX = e.clientX;
			lastY = e.clientY;

			const rotSpeed = ROT_SPEED * dist / 5;
			globe.apply_drag(dx, dy, rotSpeed);

		} else if (touches.size === 2) {
			dragging = false;
			const [p1, p2] = [...touches.values()];

			// twist (roll)
			const ang = vecAngle(p1, p2);
			if (twistLast != null) {
				let delta = ang - twistLast;
				if (delta > Math.PI) delta -= 2 * Math.PI;
				if (delta < -Math.PI) delta += 2 * Math.PI;
				globe.apply_twist(delta); // your Rust method
			}
			twistLast = ang;

			// pinch (zoom)
			const len = pinchLen(p1, p2);
			if (pinchStart) {
				const factor = pinchStart / len;
				dist = Math.min(maxDist, Math.max(minDist, pinchDist * factor));
				globe.set_distance(dist);
			}
		}
	}

	function onEnd(e: PointerEvent) {
		dragging = false;
		touches.delete(e.pointerId);
		if (touches.size < 2) {
			twistLast = null;
			pinchStart = 0;
		}
		canvas.releasePointerCapture(e.pointerId);
	}

	canvas.addEventListener('pointerdown', onDown);
	canvas.addEventListener('pointermove', onMove);
	canvas.addEventListener('pointerup', onEnd);
	canvas.addEventListener('pointercancel', onEnd);
}

interface ClientPoint {
	clientX: number
	clientY: number
}

function vecAngle(a: ClientPoint, b: ClientPoint) {
	return Math.atan2(a.clientY - b.clientY, a.clientX - b.clientX);
}

function pinchLen(a: ClientPoint, b: ClientPoint) {
	return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}

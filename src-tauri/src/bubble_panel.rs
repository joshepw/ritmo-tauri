use std::cell::Cell;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2::rc::{Allocated, Retained};
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSCompositingOperation, NSEvent, NSGraphicsContext,
    NSImage, NSPanel, NSScreen, NSScreenSaverWindowLevel, NSView, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};

const SIZE: f64 = 72.0;

struct Drag {
    start_x: Cell<f64>,
    start_y: Cell<f64>,
    origin_x: Cell<f64>,
    origin_y: Cell<f64>,
    moved: Cell<bool>,
}

impl Default for Drag {
    fn default() -> Self {
        Self {
            start_x: Cell::new(0.0),
            start_y: Cell::new(0.0),
            origin_x: Cell::new(0.0),
            origin_y: Cell::new(0.0),
            moved: Cell::new(false),
        }
    }
}

define_class!(
    #[unsafe(super(NSView))]
    #[name = "RitmoAvatarView"]
    #[ivars = Drag]
    struct AvatarView;

    impl AvatarView {
        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(this: Allocated<Self>, frame: NSRect) -> Retained<Self> {
            let this = this.set_ivars(Drag::default());
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            let path = NSBezierPath::bezierPathWithOvalInRect(bounds);
            if let Some(context) = NSGraphicsContext::currentContext() {
                context.saveGraphicsState();
                path.addClip();
                if let Some(image) = icon() {
                    let size = image.size();
                    // El PNG trae margen transparente. Se dibuja solo la marca para llenar el círculo.
                    let from = NSRect::new(
                        NSPoint::new(size.width * 21.0 / 256.0, size.height * 18.0 / 256.0),
                        NSSize::new(size.width * 214.0 / 256.0, size.height * 215.0 / 256.0),
                    );
                    image.drawInRect_fromRect_operation_fraction(
                        bounds,
                        from,
                        NSCompositingOperation::SourceOver,
                        1.0,
                    );
                }
                context.restoreGraphicsState();
            }
            let inset = 1.0;
            let ring = NSRect::new(
                NSPoint::new(inset, inset),
                NSSize::new((bounds.size.width - inset * 2.0).max(1.0), (bounds.size.height - inset * 2.0).max(1.0)),
            );
            let border = NSBezierPath::bezierPathWithOvalInRect(ring);
            NSColor::colorWithSRGBRed_green_blue_alpha(0.047, 0.047, 0.047, 1.0).setStroke();
            border.setLineWidth(2.0);
            border.stroke();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let Some(window) = self.window() else {
                return;
            };
            let loc = window.convertPointToScreen(event.locationInWindow());
            let frame = window.frame();
            let drag = self.ivars();
            drag.start_x.set(loc.x);
            drag.start_y.set(loc.y);
            drag.origin_x.set(frame.origin.x);
            drag.origin_y.set(frame.origin.y);
            drag.moved.set(false);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let Some(window) = self.window() else {
                return;
            };
            let loc = window.convertPointToScreen(event.locationInWindow());
            let drag = self.ivars();
            let dx = loc.x - drag.start_x.get();
            let dy = loc.y - drag.start_y.get();
            if dx.hypot(dy) > 4.0 {
                drag.moved.set(true);
            }
            if drag.moved.get() {
                window.setFrameOrigin(NSPoint::new(drag.origin_x.get() + dx, drag.origin_y.get() + dy));
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            let moved = self.ivars().moved.get();
            if moved {
                if let Some((x, y)) = origin() {
                    crate::shell::queue_save_bubble(x, y);
                }
            } else if let Some(app) = crate::shell::shared_app() {
                let _ = crate::shell::toggle_bubble_chat(&app);
                redraw();
            }
        }
    }
);

static PANEL: AtomicPtr<NSPanel> = AtomicPtr::new(std::ptr::null_mut());

fn with_panel<T>(body: impl FnOnce(&NSPanel) -> T) -> Option<T> {
    let ptr = PANEL.load(Ordering::Acquire);
    if ptr.is_null() {
        return None;
    }
    let panel = unsafe { Retained::retain(ptr)? };
    Some(body(&panel))
}

fn store_panel(panel: Retained<NSPanel>) {
    let raw = Retained::into_raw(panel);
    let old = PANEL.swap(raw, Ordering::AcqRel);
    if !old.is_null() {
        drop(unsafe { Retained::from_raw(old) });
    }
}

pub fn show(x: f64, y: f64) {
    if with_panel(|panel| {
        style_panel(panel);
        panel.setFrameOrigin(to_appkit(x, y));
        panel.orderFrontRegardless();
    })
    .is_some()
    {
        return;
    }
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let frame = NSRect::new(to_appkit(x, y), NSSize::new(SIZE, SIZE));
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(marker),
        frame,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe { panel.setReleasedWhenClosed(false) };
    let view: Retained<AvatarView> = unsafe {
        msg_send![
            AvatarView::alloc(marker),
            initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SIZE, SIZE))
        ]
    };
    panel.setContentView(Some(&*view));
    style_panel(&panel);
    panel.orderFrontRegardless();
    store_panel(panel);
}

pub fn hide() {
    with_panel(|panel| panel.orderOut(None));
}

pub fn move_to(x: f64, y: f64) {
    with_panel(|panel| panel.setFrameOrigin(to_appkit(x, y)));
}

pub fn origin() -> Option<(f64, f64)> {
    with_panel(|panel| to_top(panel.frame().origin))
}

pub fn lift() {
    with_panel(|panel| {
        style_panel(panel);
        panel.orderFrontRegardless();
    });
}

pub fn redraw() {
    with_panel(|panel| {
        if let Some(view) = panel.contentView() {
            view.setNeedsDisplay(true);
        }
    });
}

/// The panel only joins another app's fullscreen space if it was created
/// while Ritmo is an accessory. Recreate it after that policy is applied.
pub fn recreate_after_policy() {
    let Some((x, y)) = origin() else {
        return;
    };
    if let Some(old) = take_panel() {
        old.orderOut(None);
        drop(old);
    }
    show(x, y);
}

fn take_panel() -> Option<Retained<NSPanel>> {
    let old = PANEL.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if old.is_null() {
        None
    } else {
        unsafe { Retained::from_raw(old) }
    }
}

fn style_panel(panel: &NSPanel) {
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::CanJoinAllApplications
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle;
    panel.setCollectionBehavior(behavior);
    panel.setLevel(NSScreenSaverWindowLevel);
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(true);
    panel.setWorksWhenModal(true);
    panel.setOpaque(false);
    panel.setHasShadow(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setMovable(false);
}

fn icon() -> Option<Retained<NSImage>> {
    static ICON: AtomicPtr<NSImage> = AtomicPtr::new(std::ptr::null_mut());
    let existing = ICON.load(Ordering::Acquire);
    if !existing.is_null() {
        return unsafe { Retained::retain(existing) };
    }
    let bytes: &'static [u8] = include_bytes!("../../ui/assets/ritmo.png");
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    let raw = Retained::into_raw(image);
    if ICON.compare_exchange(std::ptr::null_mut(), raw, Ordering::AcqRel, Ordering::Acquire).is_err() {
        drop(unsafe { Retained::from_raw(raw) });
    }
    unsafe { Retained::retain(ICON.load(Ordering::Acquire)) }
}

fn to_appkit(x: f64, y_top: f64) -> NSPoint {
    NSPoint::new(x, primary_height() - y_top - SIZE)
}

fn to_top(origin: NSPoint) -> (f64, f64) {
    (origin.x, primary_height() - origin.y - SIZE)
}

fn primary_height() -> f64 {
    let Some(marker) = MainThreadMarker::new() else {
        return 900.0;
    };
    let screens = NSScreen::screens(marker);
    if screens.count() == 0 {
        return 900.0;
    }
    screens.objectAtIndex(0).frame().size.height
}

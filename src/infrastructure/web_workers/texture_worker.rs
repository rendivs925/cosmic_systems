use bevy::prelude::{Assets, Handle};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::texture::Image;
use js_sys::Reflect;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, ImageBitmap, MessageEvent, Worker,
};

pub struct TextureDecodeWorker {
    enabled: bool,
    worker: Option<Worker>,
    callback: Option<Closure<dyn FnMut(MessageEvent)>>,
    results: Rc<RefCell<VecDeque<TextureWorkerResult>>>,
    inflight: HashSet<String>,
    cache: HashMap<String, Handle<Image>>,
    canvas: Option<HtmlCanvasElement>,
    context: Option<CanvasRenderingContext2d>,
    base_url: Option<String>,
}

pub struct TextureWorkerResult {
    pub path: String,
    pub bitmap: Option<ImageBitmap>,
    pub error: Option<String>,
}

impl TextureDecodeWorker {
    pub fn new() -> Self {
        let results = Rc::new(RefCell::new(VecDeque::new()));
        let mut worker = TextureDecodeWorker {
            enabled: false,
            worker: None,
            callback: None,
            results,
            inflight: HashSet::new(),
            cache: HashMap::new(),
            canvas: None,
            context: None,
            base_url: web_sys::window()
                .and_then(|window| window.location().href().ok()),
        };

        let canvas = match create_canvas() {
            Ok(canvas) => canvas,
            Err(_) => return worker,
        };
        let context = match canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|ctx| ctx.dyn_into::<CanvasRenderingContext2d>().ok())
        {
            Some(context) => context,
            None => return worker,
        };

        let worker_handle = match create_worker() {
            Ok(worker) => worker,
            Err(err) => {
                web_sys::console::error_1(&err);
                return worker;
            }
        };

        let results = Rc::clone(&worker.results);
        let callback = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event| {
            let data = event.data();
            let path = Reflect::get(&data, &JsValue::from_str("path"))
                .ok()
                .and_then(|value| value.as_string());
            let bitmap_value = Reflect::get(&data, &JsValue::from_str("bitmap")).ok();
            let bitmap = bitmap_value
                .and_then(|value| value.dyn_into::<ImageBitmap>().ok());
            let error = Reflect::get(&data, &JsValue::from_str("error"))
                .ok()
                .and_then(|value| value.as_string());

            let Some(path) = path else {
                return;
            };

            results.borrow_mut().push_back(TextureWorkerResult { path, bitmap, error });
        }));

        worker_handle.set_onmessage(Some(callback.as_ref().unchecked_ref()));

        worker.enabled = true;
        worker.worker = Some(worker_handle);
        worker.callback = Some(callback);
        worker.canvas = Some(canvas);
        worker.context = Some(context);
        worker
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn cached_handle(&self, path: &str) -> Option<Handle<Image>> {
        if let Some(handle) = self.cache.get(path) {
            return Some(handle.clone());
        }
        let normalized = normalize_asset_path(path);
        self.cache.get(&normalized).cloned()
    }

    pub fn request(&mut self, path: &str) {
        if !self.enabled {
            return;
        }
        let resolved_path = normalize_asset_path(path);
        if self.inflight.contains(&resolved_path) || self.cache.contains_key(&resolved_path) {
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let message = js_sys::Object::new();
        let _ = Reflect::set(
            &message,
            &JsValue::from_str("path"),
            &JsValue::from_str(&resolved_path),
        );
        if let Some(base_url) = &self.base_url {
            let _ = Reflect::set(
                &message,
                &JsValue::from_str("base_url"),
                &JsValue::from_str(base_url),
            );
        }
        if worker.post_message(&message).is_ok() {
            self.inflight.insert(resolved_path);
        }
    }

    pub fn take_results(&mut self) -> Vec<TextureWorkerResult> {
        self.results.borrow_mut().drain(..).collect()
    }

    pub fn cache_image(
        &mut self,
        path: String,
        image: Image,
        images: &mut Assets<Image>,
    ) -> Handle<Image> {
        let handle = images.add(image);
        self.cache.insert(path.clone(), handle.clone());
        self.inflight.remove(&path);
        handle
    }

    pub fn mark_failed(&mut self, path: &str) {
        self.inflight.remove(path);
    }

    pub fn decode_bitmap(&self, bitmap: &ImageBitmap) -> Result<Image, JsValue> {
        let Some(canvas) = self.canvas.as_ref() else {
            return Err(JsValue::from_str("No canvas available"));
        };
        let Some(context) = self.context.as_ref() else {
            return Err(JsValue::from_str("No canvas context available"));
        };

        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        canvas.set_width(width);
        canvas.set_height(height);
        context.draw_image_with_image_bitmap(bitmap, 0.0, 0.0)?;
        let image_data = context.get_image_data(0.0, 0.0, width as f64, height as f64)?;
        let pixels = image_data.data().0;
        bitmap.close();

        let size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        Ok(Image::new_fill(
            size,
            TextureDimension::D2,
            &pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        ))
    }
}

fn create_canvas() -> Result<HtmlCanvasElement, JsValue> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| JsValue::from_str("Missing document"))?;
    let element = document.create_element("canvas")?;
    Ok(element.dyn_into::<HtmlCanvasElement>()?)
}

fn create_worker() -> Result<Worker, JsValue> {
    let script = r#"
        self.onmessage = async function(e) {
            const path = e.data.path;
            const baseUrl = e.data.base_url || "";
            try {
                const url = new URL(path, baseUrl || self.location.href);
                const response = await fetch(url);
                if (!response.ok) {
                    throw new Error(`HTTP ${response.status} ${response.statusText}`);
                }
                const contentType = response.headers.get("content-type") || "";
                if (!contentType.startsWith("image/")) {
                    const body = await response.text();
                    throw new Error(`Non-image response (${contentType}): ${body.slice(0, 120)}`);
                }
                const blob = await response.blob();
                const bitmap = await createImageBitmap(blob);
                self.postMessage({ path, bitmap }, [bitmap]);
            } catch (err) {
                self.postMessage({ path, error: err ? err.toString() : "error" });
            }
        };
    "#;

    let blob_parts = js_sys::Array::of1(&JsValue::from_str(script));
    let options = {
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("application/javascript");
        options
    };
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&blob_parts.into(), &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    Worker::new(&url)
}

fn normalize_asset_path(path: &str) -> String {
    if path.starts_with("assets/")
        || path.starts_with('/')
        || path.starts_with("http://")
        || path.starts_with("https://")
    {
        return path.to_string();
    }
    format!("assets/{}", path)
}

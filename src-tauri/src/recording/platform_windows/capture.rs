// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::Instant;

use windows::core::{factory, IInspectable, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
  Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
  D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
  D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::{
  CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::writer::{Command, Frame};

#[derive(Clone, Copy)]
pub(super) enum CaptureTarget {
  Monitor(u32),
  Window(u32),
}

impl CaptureTarget {
  fn item(self) -> Result<GraphicsCaptureItem, String> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
      .map_err(|error| error.to_string())?;
    unsafe {
      match self {
        Self::Monitor(id) => interop.CreateForMonitor(HMONITOR(id as usize as *mut c_void)),
        Self::Window(id) => interop.CreateForWindow(HWND(id as usize as *mut c_void)),
      }
    }
    .map_err(|error| error.to_string())
  }
}

pub(super) fn target_size(target: CaptureTarget) -> Result<(u32, u32), String> {
  let size = target.item()?.Size().map_err(|error| error.to_string())?;
  let width = u32::try_from(size.Width).unwrap_or_default() & !1;
  let height = u32::try_from(size.Height).unwrap_or_default() & !1;
  if width < 2 || height < 2 {
    return Err("The selected capture source has no recordable area".to_owned());
  }
  Ok((width, height))
}

pub(super) fn create_device() -> Result<ID3D11Device, String> {
  let mut device = None;
  unsafe {
    D3D11CreateDevice(
      None,
      D3D_DRIVER_TYPE_HARDWARE,
      HMODULE::default(),
      D3D11_CREATE_DEVICE_BGRA_SUPPORT,
      None,
      D3D11_SDK_VERSION,
      Some(&mut device),
      None,
      None,
    )
  }
  .map_err(|error| error.to_string())?;
  device.ok_or_else(|| "Direct3D did not create a recording device".to_owned())
}

pub(super) struct CaptureObjects {
  closed: bool,
  frame_pool: Direct3D11CaptureFramePool,
  frame_token: i64,
  session: GraphicsCaptureSession,
}

impl CaptureObjects {
  pub(super) fn start(
    device: ID3D11Device,
    target: CaptureTarget,
    width: u32,
    height: u32,
    show_cursor: bool,
    commands: SyncSender<Command>,
  ) -> Result<Self, String> {
    let item = target.item()?;
    let size = SizeInt32 {
      Width: i32::try_from(width).map_err(|_| "The capture width is too large")?,
      Height: i32::try_from(height).map_err(|_| "The capture height is too large")?,
    };
    let dxgi = device
      .cast::<IDXGIDevice>()
      .map_err(|error| error.to_string())?;
    let inspectable =
      unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(|error| error.to_string())?;
    let winrt_device = inspectable
      .cast::<IDirect3DDevice>()
      .map_err(|error| error.to_string())?;
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
      &winrt_device,
      DirectXPixelFormat::B8G8R8A8UIntNormalized,
      3,
      size,
    )
    .map_err(|error| error.to_string())?;

    let frame_token = frame_pool
      .FrameArrived(
        &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |pool, _| {
          let Some(pool) = pool.as_ref() else {
            return Ok(());
          };
          let frame = pool.TryGetNextFrame()?;
          let source_100ns = frame.SystemRelativeTime()?.Duration;
          let surface = frame.Surface()?;
          let access = surface.cast::<IDirect3DDxgiInterfaceAccess>()?;
          let texture = unsafe { access.GetInterface::<ID3D11Texture2D>()? };
          let command = Command::Frame(Frame {
            source_100ns,
            texture,
            wall: Instant::now(),
          });
          match commands.try_send(command) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => return Ok(()),
          }
          Ok(())
        }),
      )
      .map_err(|error| error.to_string())?;
    let session = frame_pool
      .CreateCaptureSession(&item)
      .map_err(|error| error.to_string())?;
    session
      .SetIsCursorCaptureEnabled(show_cursor)
      .map_err(|error| error.to_string())?;
    let _ = session.SetIsBorderRequired(false);
    session.StartCapture().map_err(|error| error.to_string())?;

    Ok(Self {
      closed: false,
      frame_pool,
      frame_token,
      session,
    })
  }

  pub(super) fn close(&mut self) {
    if self.closed {
      return;
    }
    self.closed = true;
    let _ = self.frame_pool.RemoveFrameArrived(self.frame_token);
    let _ = self.session.Close();
    let _ = self.frame_pool.Close();
  }
}

impl Drop for CaptureObjects {
  fn drop(&mut self) {
    self.close();
  }
}

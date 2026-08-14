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
use windows::Win32::Foundation::HMODULE;
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
    monitor_id: u32,
    show_cursor: bool,
    commands: SyncSender<Command>,
  ) -> Result<Self, String> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
      .map_err(|error| error.to_string())?;
    let monitor = HMONITOR(monitor_id as usize as *mut c_void);
    let item = unsafe { interop.CreateForMonitor::<GraphicsCaptureItem>(monitor) }
      .map_err(|error| error.to_string())?;
    let size = item.Size().map_err(|error| error.to_string())?;
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

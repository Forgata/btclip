use std::future::IntoFuture;
use tokio::sync::mpsc;
use windows::Devices::Bluetooth::Rfcomm::{RfcommServiceId, RfcommServiceProvider};
use windows::Foundation::TypedEventHandler;
use windows::Networking::Sockets::{
    StreamSocket, StreamSocketListener, StreamSocketListenerConnectionReceivedEventArgs,
};
use windows::Storage::Streams::{DataReader, DataWriter, InputStreamOptions};
use windows::core::Ref;

pub struct BluetoothServer {
    _provider: RfcommServiceProvider,
    _listener: StreamSocketListener,
}

pub struct BluetoothReader {
    reader: DataReader,
}

pub struct BluetoothWriter {
    writer: DataWriter,
}

pub struct BluetoothConnection {
    _socket: StreamSocket,
    reader: DataReader,
    writer: DataWriter,
}

impl BluetoothServer {
    pub async fn start() -> Result<(Self, mpsc::Receiver<BluetoothConnection>), String> {
        let service_id = RfcommServiceId::SerialPort()
            .map_err(|e| format!("failed to create standard SerialPort service id: {}", e))?;

        let provider_op = RfcommServiceProvider::CreateAsync(&service_id)
            .map_err(|e| format!("failed to call createAsync: {}", e))?;

        let provider = provider_op
            .into_future()
            .await
            .map_err(|e| format!("Failed to initialize RfcommServiceProvider: {}", e))?;

        let listener = StreamSocketListener::new()
            .map_err(|e| format!("Failed to create StreamSocketListener: {}", e))?;

        let (tx, rx) = mpsc::channel::<BluetoothConnection>(100);
        let handler_tx = tx.clone();

        listener
            .ConnectionReceived(&TypedEventHandler::new(
                move |_sender, args: Ref<StreamSocketListenerConnectionReceivedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        if let Ok(socket) = args.Socket() {
                            if let Ok(connection) = BluetoothConnection::new(socket) {
                                let _ = handler_tx.blocking_send(connection);
                            }
                        }
                    }
                    Ok(())
                },
            ))
            .map_err(|e| format!("Failed to register connection handler: {}", e))?;

        let service_name = provider
            .ServiceId()
            .map_err(|e| format!("Failed to get ServiceId: {}", e))?
            .AsString()
            .map_err(|e| format!("Failed to convert ServiceId to String: {}", e))?;

        listener
            .BindServiceNameAsync(&service_name)
            .map_err(|e| format!("Failed to bind listener: {}", e))?
            .into_future()
            .await
            .map_err(|e| format!("Failed to complete listener bind: {}", e))?;

        provider
            .StartAdvertisingWithRadioDiscoverability(&listener, true)
            .map_err(|e| {
                format!(
                    "Failed setup step for StartAdvertisingWithRadioDiscoverability: {}",
                    e
                )
            })?;

        println!("📡 Bluetooth RFCOMM Server advertising successfully!");

        Ok((
            Self {
                _provider: provider,
                _listener: listener,
            },
            rx,
        ))
    }
}

impl BluetoothConnection {
    pub fn new(socket: StreamSocket) -> Result<Self, String> {
        let input_stream = socket
            .InputStream()
            .map_err(|e| format!("Failed to get InputStream: {}", e))?;
        let output_stream = socket
            .OutputStream()
            .map_err(|e| format!("Failed to get OutputStream: {}", e))?;

        let reader = DataReader::CreateDataReader(&input_stream)
            .map_err(|e| format!("Failed to create DataReader: {}", e))?;
        let writer = DataWriter::CreateDataWriter(&output_stream)
            .map_err(|e| format!("Failed to create DataWriter: {}", e))?;

        reader
            .SetInputStreamOptions(InputStreamOptions::Partial)
            .map_err(|e| format!("Failed to set InputStreamOptions: {}", e))?;

        Ok(Self {
            _socket: socket,
            reader,
            writer,
        })
    }

    pub fn into_split(self) -> (BluetoothReader, BluetoothWriter) {
        (
            BluetoothReader {
                reader: self.reader,
            },
            BluetoothWriter {
                writer: self.writer,
            },
        )
    }
}

impl BluetoothReader {
    pub async fn read_bytes(&mut self, count: u32) -> Result<Vec<u8>, String> {
        let loaded = self
            .reader
            .LoadAsync(count)
            .map_err(|e| format!("Failed LoadAsync: {}", e))?
            .into_future()
            .await
            .map_err(|e| format!("Failed to load bytes: {}", e))?;

        if loaded < count {
            return Err("Socket disconnected before reading required bytes".to_string());
        }

        let mut buffer: Vec<u8> = vec![0u8; count as usize];
        self.reader
            .ReadBytes(&mut buffer)
            .map_err(|e| format!("Failed to read bytes: {}", e))?;

        Ok(buffer)
    }
}

impl BluetoothWriter {
    pub async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .WriteBytes(bytes)
            .map_err(|e| format!("Failed to write bytes: {}", e))?;

        self.writer
            .StoreAsync()
            .map_err(|e| format!("Failed StoreAsync: {}", e))?
            .into_future()
            .await
            .map_err(|e| format!("Failed to flush bytes: {}", e))?;

        Ok(())
    }
}

//! Python wrapper over the canonical synchronous client.

use i2pr_sam_blocking::BlockingClient;
use pyo3::{exceptions::PyValueError, prelude::*};

pyo3::create_exception!(i2pr_sam, SamError, pyo3::exceptions::PyException);

fn error(error: impl std::fmt::Display) -> PyErr {
    SamError::new_err(error.to_string())
}

#[pyclass]
struct Client {
    inner: BlockingClient,
}

#[pymethods]
impl Client {
    #[new]
    fn new(endpoint: &str) -> PyResult<Self> {
        let endpoint = endpoint
            .parse()
            .map_err(|e: std::net::AddrParseError| PyValueError::new_err(e.to_string()))?;
        BlockingClient::connect_endpoint(endpoint)
            .map(|inner| Self { inner })
            .map_err(error)
    }

    fn lookup(&self, name: &str) -> PyResult<String> {
        self.inner.lookup(name).map_err(error)
    }

    fn generate_destination(&self) -> PyResult<(String, String)> {
        let generated = self.inner.generate_destination(None).map_err(error)?;
        Ok((
            generated.public().as_str().to_owned(),
            generated.secret().expose().to_owned(),
        ))
    }
}

#[pymodule]
fn i2pr_sam(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Client>()?;
    module.add("SamError", module.py().get_type::<SamError>())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn python_client_imports_and_resolves_a_name() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("HELLO VERSION"));
            writeln!(reader.get_mut(), "HELLO REPLY RESULT=OK VERSION=3.3").unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert_eq!(line, "NAMING LOOKUP NAME=example.i2p\n");
            writeln!(
                reader.get_mut(),
                "NAMING REPLY RESULT=OK NAME=example.i2p VALUE=peer-destination"
            )
            .unwrap();
        });

        Python::with_gil(|py| {
            let module = PyModule::new(py, "i2pr_sam").unwrap();
            i2pr_sam(&module).unwrap();
            let client = module
                .getattr("Client")
                .unwrap()
                .call1((endpoint,))
                .unwrap();
            let value: String = client
                .call_method1("lookup", ("example.i2p",))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(value, "peer-destination");
            let bad_endpoint = module
                .getattr("Client")
                .unwrap()
                .call1(("not-an-endpoint",))
                .unwrap_err();
            assert!(bad_endpoint.is_instance_of::<PyValueError>(py));
            let operational = module
                .getattr("Client")
                .unwrap()
                .call1(("127.0.0.1:1",))
                .unwrap_err();
            assert!(operational.is_instance_of::<SamError>(py));
        });
        server.join().unwrap();
    }
}

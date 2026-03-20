pub mod clx;
pub mod fib;
pub mod fields;
pub mod markdown;
pub mod ole;
pub mod papx;
pub mod styles;
pub mod text;

use anyhow::Result;
use markdown::Document;

pub fn parse_doc(data: &[u8]) -> Result<Document> {
    parse_doc_with_options(data, true)
}

pub fn parse_doc_with_options(data: &[u8], strip_fields: bool) -> Result<Document> {
    let streams = ole::read_ole_streams(data)?;
    let fib = fib::parse_fib(&streams.word_document)?;

    let pieces = clx::parse_clx(&streams.table, fib.fc_clx, fib.lcb_clx)?;
    let chars = text::extract_text(&streams.word_document, &pieces)?;

    let heading_styles = styles::parse_stsh(&streams.table, fib.fc_stshf, fib.lcb_stshf)?;
    let para_styles = papx::parse_papx(
        &streams.table,
        &streams.word_document,
        fib.fc_plcf_bte_papx,
        fib.lcb_plcf_bte_papx,
    )?;
    let cp_headings = papx::map_cp_to_heading(&pieces, &para_styles, &heading_styles);

    let doc = markdown::generate_markdown(
        &chars,
        fib.ccp_text,
        fib.ccp_ftn,
        fib.ccp_hdd,
        fib.ccp_atn,
        fib.ccp_edn,
        fib.ccp_txbx,
        &cp_headings,
        strip_fields,
    );

    Ok(doc)
}

// --- Python bindings via PyO3 ---

#[cfg(feature = "python")]
mod python {
    use pyo3::prelude::*;

    #[pyclass(name = "Field")]
    #[derive(Clone)]
    struct PyField {
        #[pyo3(get)]
        field_type: String,
        #[pyo3(get)]
        name: String,
        #[pyo3(get)]
        value: String,
    }

    #[pymethods]
    impl PyField {
        fn __repr__(&self) -> String {
            format!(
                "Field(field_type=\"{}\", name=\"{}\", value=\"{}\")",
                self.field_type, self.name, self.value
            )
        }
    }

    #[pyclass(name = "Document")]
    struct PyDocument {
        #[pyo3(get)]
        body_text: String,
        #[pyo3(get)]
        textboxes: Vec<String>,
        #[pyo3(get)]
        fields: Vec<PyField>,
    }

    #[pymethods]
    impl PyDocument {
        fn __repr__(&self) -> String {
            format!(
                "Document(body_text={}..., textboxes={}, fields={})",
                &self.body_text[..self.body_text.len().min(50)],
                self.textboxes.len(),
                self.fields.len()
            )
        }
    }

    #[pyfunction]
    #[pyo3(signature = (data, strip_fields=true))]
    fn parse_doc(data: &[u8], strip_fields: bool) -> PyResult<PyDocument> {
        let doc = crate::parse_doc_with_options(data, strip_fields)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let fields = doc
            .fields
            .into_iter()
            .map(|f| PyField {
                field_type: f.field_type,
                name: f.name,
                value: f.value,
            })
            .collect();
        Ok(PyDocument {
            body_text: doc.body_text,
            textboxes: doc.textboxes,
            fields,
        })
    }

    #[pymodule]
    fn unword(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PyDocument>()?;
        m.add_class::<PyField>()?;
        m.add_function(wrap_pyfunction!(parse_doc, m)?)?;
        Ok(())
    }
}

Change the way Shank generates defined type links to other programs:

- for a defined type that is defined in the current program/crate, we are usign "defined": "typeName"
- for a defined type that references an imported type from another crate, we should use "defined": {"name": "typeName", "program": "programName"}
- presumably, the program name should match the name specified in the other program IDL file and that file should be present as well.

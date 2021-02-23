# ktest-parser
ktest-parser is a utility to parse `.ktest` binaries which is the output of KLEE, into a Rust friendly struct instead.

## KTest File Format Description
The KTest binary is structured as follows, 
1. A header 
2. KLEE arguments 
3. Symbolic arguments
4. KTest objects.

The following sections describes the detailed structure. Each new section starts at byte 0 here, but since they follow each other the arguments start at byte 8 where the header left off. But it is easier to describe the structure this way.

### Header
The header describes the magic number which is either "KTEST" or "BOUT/n". Then followed
by a version of the file format.
| BYTE | NAME    | DESCRIPTION                 | LENGTH  |
|------|---------|-----------------------------|---------|
| 0..5 | HDR     | File format (default: KTEST)| 4 bytes |
| 5..8 | VERSION | File format version         | 4 bytes |

### Arguments
The arguments section describes the number of arguments and then a repeated section of
arguments where each argument is first described by a size and then its content of size length.
#### Information
| BYTE | NAME   | DESCRIPTION                 | LENGTH  |
|------|--------|-----------------------------|---------|
| 0..4 | NUMARGS| Number of arguments         | 4 bytes |

#### Argument
This is repeated for (NUMARGS) times.
| BYTE        | NAME   | DESCRIPTION      | LENGTH       |
|-------------|--------|------------------|--------------|
| 0..4        | SIZE   | Size of argument | 4 bytes      |
| 4..(4+SIZE) | ARG    | An argument      | (SIZE) bytes |        

### Symbolic arguments
Describes symbolic arguments.
| BYTE | NAME    | DESCRIPTION | LENGTH  |
|------|---------|-------------|---------|
| 0..4 | ARGVS   | none        | 4 bytes |
| 4..8 | ARGVLEN | none        | 4 bytes |

### Objects
Like the arguments section, the first item is the number of objects. Then followed by
a repeated section of objects where each object is described by a size and then its content
of size length.
#### Information
| BYTE | NAME      | DESCRIPTION       | LENGTH  |
|------|-----------|-------------------|---------|
| 0..4 | NUMOBJECTS| Number of objects | 4 bytes |

#### Object
This is repeated for (NUMOBJECTS) times.
| BYTE        | NAME   | DESCRIPTION    | LENGTH       |
|-------------|--------|----------------|--------------|
| 0..4        | SIZE   | Size of object | 4 bytes      |
| 4..(4+SIZE) | OBJECT | An object      | (SIZE) bytes |        

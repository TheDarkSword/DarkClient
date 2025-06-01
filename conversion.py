# Convert mojang mappings to JSON format readable by DarkClient
import re
import json

def convert_java_type_to_jvm(java_type, class_map):
    array_depth = 0
    while java_type.endswith("[]"):
        array_depth += 1
        java_type = java_type[:-2]

    primitive_map = {
        "void": "V",
        "boolean": "Z",
        "byte": "B",
        "char": "C",
        "short": "S",
        "int": "I",
        "float": "F",
        "long": "J",
        "double": "D"
    }

    if java_type in primitive_map:
        jvm_type = primitive_map[java_type]
    else:
        internal_name = java_type.replace('.', '/')
        obfuscated_name = class_map.get(internal_name, internal_name)
        jvm_type = f"L{obfuscated_name};"

    return ("[" * array_depth) + jvm_type

def parse_parameter(param):
    parts = param.rsplit(' ', 1)
    return parts[0] if len(parts) > 1 else param

def get_method_signature(return_type, params_str, class_map):
    params = []
    for param in params_str.split(','):
        param = param.strip()
        if not param:
            continue
        param_type = parse_parameter(param)
        params.append(convert_java_type_to_jvm(param_type, class_map))

    return_type_jvm = convert_java_type_to_jvm(return_type, class_map)
    return f"({''.join(params)}){return_type_jvm}"

def parse_mappings(input_text):
    data = {"classes": {}}
    class_map = {}

    class_re = re.compile(r'^([\w\.$]+) -> ([\w$]+):$')
    method_re = re.compile(r'^\s+(?:\d+:\d+:)?([\w\.<>$]+)\s+([\w<>$]+)\((.*)\)\s+->\s+([\w<>$]+)$')
    field_re = re.compile(r'^\s+([\w\.<>$]+)\s+([\w$]+)\s+->\s+([\w$]+)$')

    lines = [line.rstrip() for line in input_text.split('\n') if line.strip() and not line.startswith('#')]

    # First pass: proces only the classes for fill class_map
    for line in lines:
        if class_match := class_re.match(line):
            original_java = class_match.group(1)
            obfuscated_name = class_match.group(2)
            original_jvm = original_java.replace('.', '/')
            class_map[original_jvm] = obfuscated_name
            if original_jvm not in data["classes"]:
                data["classes"][original_jvm] = {
                    "name": obfuscated_name,
                    "methods": {},
                    "fields": {}
                }

    # Second pass: process methods and fields with completed class_map
    current_class = None
    for line in lines:
        # Class definition handling (only sets current_class)
        if class_match := class_re.match(line):
            original_java = class_match.group(1)
            original_jvm = original_java.replace('.', '/')
            current_class = original_jvm
            continue

        if not current_class:
            continue

        # Method handling
        if method_match := method_re.match(line):
            return_type = method_match.group(1)
            method_name = method_match.group(2)
            params = method_match.group(3)
            obf_method = method_match.group(4)

            signature = get_method_signature(return_type, params, class_map)
            data["classes"][current_class]["methods"][method_name] = {
                "name": obf_method,
                "signature": signature
            }
            continue

        # Field handling
        if field_match := field_re.match(line):
            field_type = field_match.group(1)
            field_name = field_match.group(2)
            obf_field = field_match.group(3)

            data["classes"][current_class]["fields"][field_name] = {
                "name": obf_field
            }

    return data

# Read input from file
with open('input.txt', 'r') as f:
    input_text = f.read()

# Convert in data structure
mapping_data = parse_mappings(input_text)

# Write the output in JSON
with open('output.json', 'w') as f:
    json.dump(mapping_data, f, indent=4, ensure_ascii=False)

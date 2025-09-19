import json
character_list = {}
skin_list = []


with open("aliricia_format.txt","r") as original_data:
    """
    We need to grab the data in 2 different lists. One for the character 4 letter character id
    in the parent list and all the skins trimmed in a different list. The parents always start with `**`
    so we search for that. 
    """

    # make lines consistent
    lines = [y if y.startswith("**") else y[4::] for y in [x.strip() for x in original_data.readlines()]]


    for ident in lines:
        if ident.startswith("**"):
            char_id,name = ident.removeprefix("**").removesuffix(":**").strip().split(" - ")
            character_list.update({char_id:name})

            skin_list.append({
                "name": name,
                "id": char_id,
                "skinid": f"{char_id}001",
                "skin_name": "Default"
            })

        elif len(ident) < 1:
            continue # filter out empty lines
        
        else:
            # since character list is guaranteed to be populated with char_id before skins we can iterate this
            ident = ident.strip()
            skin_id, name = ident.split(" - ")
            char_id = skin_id[0:4]
            
            print(skin_id,name, end="-- ")
            print(character_list[char_id]) 

            skin_list.append({
                "name": character_list[char_id],
                "id": char_id,
                "skinid": skin_id,
                "skin_name": name
            })

    print(skin_list)

with open("character_data.json", "w") as f:
    json.dump(skin_list, f,indent=4)